use super::*;

impl AppState {
    pub async fn register_login_attempt(
        &self,
        connection_id: &str,
        provider: Provider,
        waiting: LoginStatus,
    ) -> Result<(Account, LoginStatus, bool), String> {
        self.ensure_settings_writable()?;
        let _publish_guard = self.publish_lock.lock().await;
        let account = self.account(connection_id, provider).await?;
        let mut statuses = self.login_statuses.lock().await;
        if let Some(existing) = statuses
            .get(connection_id)
            .filter(|status| status.state == crate::models::LoginState::Waiting)
            .cloned()
        {
            return Ok((account, existing, false));
        }
        statuses.insert(connection_id.to_owned(), waiting.clone());
        Ok((account, waiting, true))
    }

    pub async fn publish_login_status_if_account_exists(
        &self,
        connection_id: &str,
        provider: Provider,
        status: LoginStatus,
    ) -> bool {
        let _publish_guard = self.publish_lock.lock().await;
        let exists = self
            .snapshot
            .read()
            .await
            .settings
            .accounts
            .iter()
            .any(|account| account.id == connection_id && account.provider == provider);
        if exists {
            self.login_statuses
                .lock()
                .await
                .insert(connection_id.to_owned(), status);
        }
        exists
    }

    /// Binds a GitHub connection to a gh account. A connection already bound to
    /// another GitHub user is rebound only when the user explicitly confirmed the
    /// change (`allow_identity_change`); automatic binding never passes it.
    pub async fn connect_github_cli_account<P: SnapshotPublisher>(
        &self,
        provider: Provider,
        connection_id: &str,
        username: &str,
        allow_identity_change: bool,
        publisher: &P,
    ) -> Result<Account, String> {
        self.ensure_settings_writable()?;
        if !matches!(provider, Provider::Github | Provider::GithubDeployments) {
            return Err("Only GitHub connections can select a GitHub CLI account.".into());
        }

        let candidate = self.account(connection_id, provider).await?;
        let selected = Account {
            cli_username: Some(username.to_owned()),
            ..candidate.clone()
        };
        selected.validate()?;
        let invocation = self
            .tools
            .invocation_for(&selected)?
            .ok_or_else(|| "GitHub CLI was not found.".to_owned())?;
        let (identity, verified_username) =
            crate::providers::github::github_cli_identity(&invocation)
                .await
                .map_err(|error| error.user_message("GitHub"))?;
        if !verified_username.eq_ignore_ascii_case(username) {
            return Err("GitHub CLI returned a different account than the selected user.".into());
        }
        let identity_changed = candidate
            .profile
            .as_deref()
            .is_some_and(|expected| expected != identity);
        if identity_changed && !allow_identity_change {
            return Err("The selected GitHub CLI user does not match this connection's saved identity. Existing targets were preserved.".into());
        }

        let bound = Account {
            profile: Some(identity),
            cli_username: Some(username.to_owned()),
            ..candidate.clone()
        };
        bound.validate()?;

        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        let current = previous
            .settings
            .accounts
            .iter()
            .find(|account| account.id == connection_id && account.provider == provider)
            .ok_or_else(|| "The selected account no longer exists.".to_owned())?;
        if current.profile != candidate.profile || current.cli_username != candidate.cli_username {
            return Err("The selected account changed while its GitHub identity was being verified. Try again.".into());
        }
        if previous.settings.accounts.iter().any(|account| {
            account.id != connection_id
                && account.provider == provider
                && (account.profile == bound.profile
                    || account
                        .cli_username
                        .as_deref()
                        .is_some_and(|value| value.eq_ignore_ascii_case(username)))
        }) {
            return Err("That GitHub CLI user is already connected for this provider.".into());
        }

        let mut settings = previous.settings.clone();
        let slot = settings
            .accounts
            .iter_mut()
            .find(|account| account.id == connection_id && account.provider == provider)
            .ok_or_else(|| "The selected account no longer exists.".to_owned())?;
        *slot = bound.clone();
        self.publish_settings_locked(settings, previous, publisher)
            .await?;
        if identity_changed {
            self.forget_connection_identity(connection_id, publisher)
                .await;
        }
        self.refresh_epoch.fetch_add(1, Ordering::SeqCst);
        Ok(bound)
    }

    /// Drops observations made under a connection's previous identity so that
    /// "my runs" and notifications are recomputed for the new identity by the
    /// next query instead of reusing the old account's ownership.
    async fn forget_connection_identity<P: SnapshotPublisher>(
        &self,
        connection_id: &str,
        publisher: &P,
    ) {
        self.history_cache
            .lock()
            .await
            .remove_connection(connection_id);
        let current = self.snapshot.read().await.clone();
        let targets: Vec<&Target> = current
            .settings
            .targets
            .iter()
            .filter(|target| target.connection_id == connection_id)
            .collect();
        let target_keys: Vec<String> = targets.iter().map(|target| target.key()).collect();
        self.notification_engine
            .lock()
            .await
            .forget_targets(&target_keys);
        // Nothing is known about the new identity yet: report the targets as
        // being checked rather than as an empty successful result.
        let attempted_at = Utc::now().to_rfc3339();
        let query_health = current
            .query_health
            .iter()
            .filter(|health| health.connection_id != connection_id)
            .cloned()
            .chain(targets.iter().map(|target| QueryHealth {
                provider: target.provider,
                connection_id: target.connection_id.clone(),
                target_id: target.id.clone(),
                status: crate::models::QueryHealthStatus::Checking,
                last_attempt_at: attempted_at.clone(),
                last_successful_at: None,
            }))
            .collect();
        let pending_targets = current
            .pending_targets
            .iter()
            .filter(|key| !target_keys.contains(key))
            .cloned()
            .chain(target_keys.iter().cloned())
            .collect();
        let updated = Snapshot {
            query_health,
            pending_targets,
            connections: current
                .connections
                .iter()
                .filter(|item| item.connection_id != connection_id)
                .cloned()
                .collect(),
            activities: current
                .activities
                .iter()
                .filter(|item| item.connection_id != connection_id)
                .cloned()
                .collect(),
            history: current
                .history
                .iter()
                .filter(|item| item.connection_id != connection_id)
                .cloned()
                .collect(),
            issues: current
                .issues
                .iter()
                .filter(|item| item.connection_id.as_deref() != Some(connection_id))
                .cloned()
                .collect(),
            ..current.clone()
        };
        let updated = with_poll_interval(updated);
        *self.snapshot.write().await = updated.clone();
        publisher.publish_snapshot(&updated);
        self.request_refresh();
    }

    pub async fn finalize_vercel_cli_account<P: SnapshotPublisher>(
        &self,
        connection_id: &str,
        publisher: &P,
    ) -> Result<Account, String> {
        self.ensure_settings_writable()?;
        let candidate = self.account(connection_id, Provider::Vercel).await?;
        let invocation = self
            .tools
            .invocation_for(&candidate)?
            .ok_or_else(|| "Vercel CLI was not found.".to_owned())?;
        let (identity, _) = crate::providers::vercel::vercel_cli_identity(&invocation)
            .await
            .map_err(|error| error.user_message("Vercel"))?;
        if candidate
            .profile
            .as_deref()
            .is_some_and(|expected| expected != identity)
        {
            return Err("The signed-in Vercel user does not match this connection's saved identity. Existing targets were preserved.".into());
        }
        let bound = Account {
            profile: Some(identity),
            ..candidate.clone()
        };
        bound.validate()?;

        let _publish_guard = self.publish_lock.lock().await;
        let previous = self.snapshot.read().await.clone();
        let current = previous
            .settings
            .accounts
            .iter()
            .find(|account| account.id == connection_id && account.provider == Provider::Vercel)
            .ok_or_else(|| "The selected account no longer exists.".to_owned())?;
        if current.profile != candidate.profile {
            return Err("The selected account changed while its Vercel identity was being verified. Try again.".into());
        }
        if previous.settings.accounts.iter().any(|account| {
            account.id != connection_id
                && account.provider == Provider::Vercel
                && account.profile == bound.profile
        }) {
            return Err("That Vercel user is already connected.".into());
        }
        let mut settings = previous.settings.clone();
        let slot = settings
            .accounts
            .iter_mut()
            .find(|account| account.id == connection_id && account.provider == Provider::Vercel)
            .ok_or_else(|| "The selected account no longer exists.".to_owned())?;
        *slot = bound.clone();
        self.publish_settings_locked(settings, previous, publisher)
            .await?;
        self.refresh_epoch.fetch_add(1, Ordering::SeqCst);
        Ok(bound)
    }
}
