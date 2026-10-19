import type { FeatureLocale } from "./featureCopy";
const text = {
  title: [
    "상태바 표시 설정",
    "Menu bar display",
    "メニューバーの表示",
    "菜单栏显示",
  ],
  help: [
    "상태바에 표시할 서비스와 프로젝트를 선택하세요.",
    "Choose the services and projects shown in the menu bar.",
    "メニューバーに表示するサービスとプロジェクトを選択します。",
    "选择菜单栏中显示的服务和项目。",
  ],
  scope: [
    "표시 설정은 알림 설정과 별개입니다. 실행자 범위는 실행 현황의 ‘내 실행만 / 전체 실행’ 설정을 따릅니다.",
    "Display settings do not affect notifications. Actor scope follows My runs / All runs in the dashboard.",
    "表示設定は通知設定とは別です。実行者の範囲はダッシュボードの設定に従います。",
    "显示设置与通知设置相互独立，执行者范围跟随仪表板的筛选设置。",
  ],
  iconPlacement: [
    "macOS에서는 ⌘ 키를 누른 채 상태 막대 아이콘을 드래그해 위치를 옮길 수 있습니다.",
    "On macOS, hold Command and drag the menu bar icon to move it.",
    "macOSではCommandキーを押したままメニューバーのアイコンをドラッグすると位置を変えられます。",
    "在 macOS 上，按住 Command 键并拖动菜单栏图标即可调整位置。",
  ],
  icon: [
    "메뉴 막대 아이콘",
    "Menu bar icon",
    "メニューバーのアイコン",
    "菜单栏图标",
  ],
  animateIcon: [
    "진행 중일 때 메뉴 막대 아이콘 움직이기",
    "Animate the menu bar icon while runs are in progress",
    "実行中はメニューバーのアイコンを動かす",
    "有运行进行中时让菜单栏图标动起来",
  ],
  animateIconHelp: [
    "macOS에서 라텔이 걷습니다. 시스템의 ‘동작 줄이기’가 켜져 있으면 멈춰 있습니다.",
    "On macOS the ratel walks. It stays still when the system's Reduce motion setting is on.",
    "macOSではラーテルが歩きます。システムの「視差効果を減らす」がオンのときは止まったままです。",
    "在 macOS 上蜜獾会走动。系统开启“减弱动态效果”时保持静止。",
  ],
  tips: [
    "상태 막대 도움말",
    "Menu bar tip",
    "メニューバーのヒント",
    "菜单栏提示",
  ],
  services: [
    "표시할 서비스",
    "Visible services",
    "表示するサービス",
    "显示的服务",
  ],
  projects: [
    "표시할 프로젝트",
    "Visible projects",
    "表示するプロジェクト",
    "显示的项目",
  ],
  noTargets: [
    "‘연결 및 설정’에서 모니터링 대상을 먼저 추가해 주세요.",
    "Add a monitored project in Connections first.",
    "先に「接続と設定」で監視対象を追加してください。",
    "请先在“连接与设置”中添加监控项目。",
  ],
  sections: ["표시 항목", "Sections", "表示項目", "显示内容"],
  active: [
    "진행 중 표시",
    "Show active runs",
    "実行中を表示",
    "显示进行中的运行",
  ],
  completed: [
    "최근 종료 표시",
    "Show completed runs",
    "完了した実行を表示",
    "显示已完成的运行",
  ],
  limit: [
    "섹션별 표시 건수",
    "Rows per section",
    "セクションごとの表示件数",
    "每个分区的显示条数",
  ],
  save: ["설정 저장", "Save settings", "設定を保存", "保存设置"],
  saved: [
    "설정을 저장했습니다.",
    "Settings saved.",
    "設定を保存しました。",
    "设置已保存。",
  ],
  exclusions: [
    "상태 미확인과 조회 오류는 따로 표시합니다. 오래 진행 중인 실행도 마지막 확인 상태와 함께 유지합니다. Vercel BLOCKED는 제외합니다.",
    "Unknown states and retrieval errors stay visible. Long-running items retain their last observed state. Vercel BLOCKED is excluded.",
    "状態不明と取得エラーは別途表示します。長時間実行中の項目も最後に確認した状態で残ります。Vercel BLOCKEDは除外します。",
    "状态未知和查询错误会单独显示。长时间运行的项目保留最后一次观察到的状态。Vercel BLOCKED 将被排除。",
  ],
} as const;
export const panelCopy = (locale: FeatureLocale, key: keyof typeof text) =>
  text[key][({ ko: 0, en: 1, ja: 2, "zh-CN": 3 } as const)[locale]];
