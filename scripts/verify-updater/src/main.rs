use std::{env, fs, path::Path};

use base64::{engine::general_purpose::STANDARD, Engine};
use minisign_verify::{PublicKey, Signature};

fn decode(value: &str) -> Result<String, &'static str> {
    let bytes = STANDARD
        .decode(value.trim())
        .map_err(|_| "Invalid base64 encoding.")?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8 encoding.")
}

fn public_key(value: &str) -> Result<PublicKey, &'static str> {
    PublicKey::decode(&decode(value)?).map_err(|_| "Invalid updater public key.")
}

fn verify(key: &PublicKey, artifact: &[u8], signature: &str) -> Result<(), &'static str> {
    let signature =
        Signature::decode(&decode(signature)?).map_err(|_| "Invalid updater signature.")?;
    key.verify(artifact, &signature, true)
        .map_err(|_| "Updater signature verification failed.")
}

fn verify_directory(key: &PublicKey, directory: &Path) -> Result<usize, &'static str> {
    let entries = fs::read_dir(directory).map_err(|_| "Cannot read bundle directory.")?;
    let mut count = 0;
    for entry in entries {
        let entry = entry.map_err(|_| "Cannot read bundle entry.")?;
        let kind = entry
            .file_type()
            .map_err(|_| "Cannot inspect bundle entry.")?;
        if kind.is_symlink() {
            continue;
        }
        let path = entry.path();
        if kind.is_dir() {
            count += verify_directory(key, &path)?;
        } else if path.extension().is_some_and(|extension| extension == "sig") {
            let artifact_path = path.with_extension("");
            let metadata = fs::symlink_metadata(&artifact_path)
                .map_err(|_| "Signature has no readable artifact.")?;
            if !metadata.is_file() {
                return Err("Signed artifact must be a regular file.");
            }
            let artifact = fs::read(artifact_path).map_err(|_| "Cannot read signed artifact.")?;
            let signature =
                fs::read_to_string(path).map_err(|_| "Cannot read updater signature.")?;
            verify(key, &artifact, &signature)?;
            count += 1;
        }
    }
    Ok(count)
}

fn run() -> Result<(), &'static str> {
    let value = env::var("RATELLINE_UPDATER_PUBLIC_KEY")
        .map_err(|_| "RATELLINE_UPDATER_PUBLIC_KEY is required.")?;
    let key = public_key(&value)?;
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() != 1 {
        return Err("Usage: verify-updater --key-only | BUNDLE_DIRECTORY");
    }
    if arguments[0] == "--key-only" {
        println!("Updater public key is valid.");
        return Ok(());
    }
    let count = verify_directory(&key, Path::new(&arguments[0]))?;
    if count == 0 {
        return Err("No signed updater artifacts found.");
    }
    println!("Verified {count} updater artifact signatures.");
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Public upstream minisign-verify test vector; no private signing material.
    const KEY: &str = "untrusted comment: test public key\nRWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3";
    const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\nRWQf6LRCGA9i59SLOFxz6NxvASXDJeRtuZykwQepbDEGt87ig1BNpWaVWuNrm73YiIiJbq71Wi+dP9eKL8OC351vwIasSSbXxwA=\ntrusted comment: timestamp:1555779966\tfile:test\nQtKMXWyYcwdpZAlPF7tE2ENJkRd1ujvKjlj1m9RtHTBnZPa5WKU5uWRs5GoP5M/VqE81QFuMKI5k/SfNQUaOAA==";

    #[test]
    fn rejects_empty_and_malformed_keys() {
        for input in ["", "not a key", "c2VjcmV0"] {
            assert!(public_key(input).is_err());
        }
    }

    #[test]
    fn scans_nested_artifacts_and_rejects_orphan_signatures() {
        let directory = env::temp_dir().join(format!("ratelline-verifier-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let key = public_key(&STANDARD.encode(KEY)).unwrap();
        assert_eq!(verify_directory(&key, &directory).unwrap(), 0);
        let nested = directory.join("bundle");
        fs::create_dir(&nested).unwrap();
        fs::write(nested.join("test.tar.gz"), b"test").unwrap();
        fs::write(nested.join("test.tar.gz.sig"), STANDARD.encode(SIGNATURE)).unwrap();
        assert_eq!(verify_directory(&key, &directory).unwrap(), 1);
        fs::remove_file(nested.join("test.tar.gz")).unwrap();
        assert!(verify_directory(&key, &directory).is_err());
        fs::remove_file(nested.join("test.tar.gz.sig")).unwrap();
        fs::remove_dir(nested).unwrap();
        fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn verifies_artifact_and_rejects_tampering_and_wrong_key() {
        let key = public_key(&STANDARD.encode(KEY)).unwrap();
        let signature = STANDARD.encode(SIGNATURE);
        assert!(verify(&key, b"test", &signature).is_ok());
        assert!(verify(&key, b"Test", &signature).is_err());
        assert!(verify(&key, b"test", "invalid").is_err());
        let wrong = KEY.replace("RWQf", "RWQg");
        let wrong_key = public_key(&STANDARD.encode(wrong)).unwrap();
        assert!(verify(&wrong_key, b"test", &signature).is_err());
    }
}
