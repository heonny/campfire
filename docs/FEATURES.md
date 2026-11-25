# Campfire Feature Map

Verified at `79f07fc` · `2026-11-25` · Structural view: none, the Architecture section of `CLAUDE.md` covers it
Premises: 단일 데스크톱 앱이라 서버나 API hop이 없습니다. 상태는 `src/main.rs` → `apply_action`에서만 바뀌고, UI는 `ui::View`를 읽고 `ui::Action`을 보냅니다. UI 문구는 모두 영어 하드코딩이며 i18n이 없습니다. 사용자는 한국어로 증상을 말해도 화면에서는 영어 문구를 봅니다.

## How to use this map

On a vague report such as "something in the sidebar is broken" or "the memo field won't take input":

1. **Where** did it happen? → Screen index → candidate feature IDs.
2. **What words** did they use? → Alias index → narrow the candidates. An alias marked `(ambiguous)` maps to several features, so check each, or ask one question that tells them apart. A row marked `→ none` is something the product does not have.
3. **What text** did they see? → grep the literal strings listed in the entry.
4. Open the entry's code path top-down; read **Pitfalls** before debugging.
5. No match anywhere → search the code, then add the missing feature, alias, or string here.

## Screen index

| Screen / region | Feature IDs |
|---|---|
| 왼쪽 사이드바 헤더 (Projects, i, 사이드바, + 아이콘) | `project-sidebar`, `sidebar-collapse`, `help-dialog`, `project-editor` |
| 사이드바 프로젝트 카드 | `project-sidebar`, `project-reorder`, `resource-metrics`, `log-dock` |
| 카드 우클릭 메뉴 | `project-context-menu`, `server-lifecycle`, `delete-project` |
| 접힌 사이드바 (상태 점 rail) | `sidebar-collapse` |
| 오른쪽 로그 영역 상단 탭 줄 | `workspace-tabs` |
| 로그 pane 헤더 (상태, 포트, Run/Stop/Restart/Edit/닫기) | `log-dock`, `server-lifecycle`, `port-check` |
| 로그 본문과 하단 툴바 | `log-view`, `log-search` |
| 프로젝트 추가/편집 창 | `project-editor`, `launch-detection`, `env-and-shell`, `port-check`, `delete-project` |
| 삭제 확인 창 | `delete-project` |
| 화면 하단 알림 배너 | `ready-crash-toast`, `port-check`, `session-recovery`, `config-storage` |
| Help 창 | `help-dialog` |
| 앱 실행 직후 (시작 시 다이얼로그, 복구 알림) | `session-recovery` |

## Alias index

| User says | Feature IDs |
|---|---|
| 프로젝트 목록, 카드, 서버 목록, sidebar, 이름 잘림, 이름이 포트와 겹침 | `project-sidebar` |
| 순서 바꾸기, 드래그 정렬, reorder, 카드가 튐 | `project-reorder` |
| 드래그 `(ambiguous)` | `project-reorder`, `log-dock` |
| 우클릭, 메뉴, 브라우저로 열기, 폴더 열기, 명령 복사, 복제 | `project-context-menu` |
| 사이드바 숨기기, 접기, Cmd+B | `sidebar-collapse` |
| 프로젝트 추가, 편집, 설정 창, 저장, 취소, 설정이 반영 안 됨 | `project-editor` |
| + 버튼 `(ambiguous)` | `project-editor`, `workspace-tabs` |
| 자동 인식, 명령 자동 입력, bootRun, gradle task, script 선택, cargo, go run, 명령이 바뀜, --no-daemon 사라짐 | `launch-detection` |
| 환경 변수, .env, env, shell, nvm, sdkman, PATH, 명령을 못 찾음 | `env-and-shell` |
| 포트, 포트 충돌, port in use, 이미 사용 중 | `port-check` |
| 시작, 실행, 중지, 재시작, 안 꺼짐, 강제 종료, restart, stop | `server-lifecycle` |
| 재시작 실패 `(ambiguous)` | `server-lifecycle`, `port-check` |
| Ready 알림, 크래시 알림, 알림, 토스트, 배너, 알림이 합쳐짐 | `ready-crash-toast` |
| 바로 up 됨, 0.0s Ready `(ambiguous)` | `port-check`, `ready-crash-toast` |
| 삭제, 프로젝트 지우기 | `delete-project` |
| 탭, 워크스페이스, 탭 이름 변경, Cmd+W, Cmd+1, 탭이 안 보임 | `workspace-tabs` |
| 로그 창 분할, 화면 분할, 로그 4개, 다섯 번째 로그, 로그 창 닫기, 업타임 | `log-dock` |
| 닫기 `(ambiguous)` | `log-dock`, `workspace-tabs`, `project-editor` |
| 로그, 로그가 안 보임, 로그가 비어 보임, 로그 사라짐, 색상, ANSI, 자동 스크롤, follow, 줄바꿈, 지우기, 빨간 경고, id clash | `log-view` |
| 로그 검색, 찾기, 필터, 필터가 안 됨, 정규식, Cmd+F | `log-search` |
| 검색 `(ambiguous)` | `log-search`, `launch-detection` |
| CPU, 메모리, 리소스 사용량, CPU 100% 넘음 | `resource-metrics` |
| 이미 실행 중, 두 번 실행, 복구, 고아 프로세스, orphan, 재실행 후 서버 남음 | `session-recovery` |
| 복구 후 로그 없음 `(ambiguous)` | `session-recovery`, `log-view` |
| 설정 파일, servers.toml, 저장 실패 | `config-storage` |
| 도움말, 버전, 단축키 | `help-dialog` |
| 로그 저장, 로그 내보내기, 로그 파일 | → none; nearest: `log-view` |
| 로그 pane의 복사 버튼 | → none; nearest: `log-view` |
| 다크 모드 | → none |
| 로그인 시 자동 시작, 부팅 시 서버 자동 실행 | → none; nearest: `session-recovery` |
| 워크스페이스 저장, 탭 복원 | → none; nearest: `workspace-tabs` |
| 자동 업데이트, 업데이트 확인 | → none; nearest: `help-dialog` |

## Features

### `project-sidebar`: 왼쪽에 프로젝트 카드 목록을 보여 주고 클릭하면 로그를 띄움

- **Where**: 왼쪽 사이드바
- **Aliases**: 프로젝트 목록, 카드, 서버 목록 (inferred)
- **UI strings**: "Projects" (untranslated), "No projects yet — press + to add one." (untranslated), "Add project" (untranslated)
- **Controls**: 카드 클릭 → `Action::ShowLog`, "Add project" (+ 아이콘) → `Action::OpenNew`, ":{port}" 링크 → `port_link`
- **Code path**:
  - UI: `src/ui/server_list.rs` → `render_card`
  - Handler: `src/main.rs` → `apply_action`, 링크는 `src/system.rs` → `open_url`
  - Data: `CampfireApp.servers`, `src/store.rs`에서 로드
- **Tests**: `src/ui/server_list.rs`
- **Pitfalls**: 좁은 사이드바에서 긴 이름이 포트와 겹침 → 이름에 `Label::truncate`가 없었음, source: `5901c04`
- **Related**: `project-reorder`, `resource-metrics`

### `project-reorder`: 카드를 드래그해서 순서를 바꾸고 저장

- **Where**: 사이드바 카드 목록, 놓을 위치에 accent 가로선
- **Aliases**: 순서 바꾸기, 드래그 정렬 (inferred)
- **UI strings**: none, 표시는 가로선뿐
- **Controls**: 카드 드래그 → `Action::Reorder`
- **Code path**:
  - UI: `src/ui/server_list.rs` → `show`, 드래그 상태는 `src/ui/mod.rs` → `SidebarDrag`
  - Handler: `src/main.rs` → `reorder_servers`, `src/main.rs` → `move_in_place`
  - Data: `servers.toml`의 `servers` 순서가 곧 화면 순서, `src/main.rs` → `persist`
- **Tests**: `src/ui/server_list_drag_tests.rs`, `src/main.rs`
- **Pitfalls**: 드래그 중 카드가 튀거나 dock에 놓았는데 순서가 바뀜 → 불안정한 widget ID와 dock 위 release를 reorder로 처리, source: `d9e7487`
- **Shares**: 같은 드래그가 dock에서는 `log-dock`의 분할로 쓰임
- **Related**: `log-dock`

### `project-context-menu`: 카드 우클릭 메뉴

- **Where**: 사이드바 카드 → 우클릭
- **Aliases**: 우클릭 메뉴, 브라우저로 열기, 폴더 열기, 명령 복사, 복제 (inferred)
- **UI strings**: "Open in browser", "Reveal working dir", "Copy command", "Open log", "Duplicate", "Edit", "Delete" (all untranslated)
- **Controls**: "Start" / "Stop" / "Restart" → `Action::Start` / `Action::Stop` / `Action::Restart`, "Open in browser" → `open_url`, "Reveal working dir" → `reveal_dir`, "Copy command" → `copy_text`, "Open log" → `Action::OpenLog`, "Duplicate" → `Action::Duplicate`, "Edit" → `Action::OpenEdit`, "Delete" → `Action::Delete`
- **Code path**:
  - UI: `src/ui/server_list.rs` → `card_context_menu`
  - OS 연동: `src/system.rs` → `reveal_dir`
  - Handler: `src/main.rs` → `apply_action`
- **Tests**: none
- **Pitfalls**: 메뉴에 "Open in browser"가 없음 → 포트가 설정된 프로젝트에만 표시, source: `src/ui/server_list.rs` → `card_context_menu`
- **Related**: `server-lifecycle`, `delete-project`, `project-editor`

### `sidebar-collapse`: 사이드바를 상태 점 rail로 접고 펴기

- **Where**: 사이드바 헤더의 사이드바 아이콘, 접으면 왼쪽 rail
- **Aliases**: 사이드바 숨기기, 접기 (inferred)
- **UI strings**: "Hide sidebar (Cmd/Ctrl+B)", "Show sidebar" (untranslated)
- **Controls**: "Hide sidebar (Cmd/Ctrl+B)" → `Action::ToggleSidebar`, rail 점 클릭 → `Action::ShowLog`
- **Code path**:
  - UI: `src/ui/server_list.rs` → `rail`
  - Handler: `src/main.rs` → `apply_action`, 상태는 `sidebar_collapsed`
- **Tests**: none
- **Pitfalls**: none
- **Related**: `project-sidebar`

### `project-editor`: 프로젝트 추가와 편집 폼

- **Where**: + 아이콘 또는 Edit → 모달
- **Aliases**: 프로젝트 추가, 편집, 설정 창 (inferred)
- **UI strings**: "Add project", "Edit project", "Project name", "Working directory", "Choose a folder…", "Preset", "Command", "Port (optional)", "Changes apply the next time this project starts or restarts.", "Discard your unsaved changes?", "Enter a project name.", "Choose a working directory.", "This directory does not exist.", "Enter a command or select a script.", "Enter a port from 1 to 65535.", "Launch summary" (all untranslated)
- **Controls**: "Save" → `EditorOutcome::Save`, "Cancel" → `request_close`, "Discard changes" / "Keep editing" → `request_close`, "Browse…" → `rfd` 폴더 선택, "Delete project…" → `EditorOutcome::Delete`, Cmd/Ctrl+Enter → 저장
- **Code path**:
  - UI: `src/ui/editor/form.rs` → `show`
  - State: `src/ui/editor.rs` → `to_config`, 검증은 `src/ui/editor/session.rs` → `field_error`, `src/model.rs` → `validate`
  - Handler: `src/main.rs` → `apply_editor_outcome`
  - Data: `src/main.rs` → `persist` → `servers.toml`
- **Tests**: `src/ui/editor/form_tests.rs`, `src/ui/editor/tests.rs`, `src/ui/editor/session.rs`, `src/model.rs`
- **Pitfalls**: 실행 중인 서버에서 포트나 env를 바꿨는데 반영되지 않음 → 설정은 다음 start 때 spawn에 적용됨, source: `src/ui/editor/form.rs` → `show`
- **Related**: `launch-detection`, `env-and-shell`, `port-check`

### `launch-detection`: 폴더를 고르면 이름과 실행 명령을 자동으로 채움

- **Where**: 프로젝트 추가 창 → Working directory 선택 직후, Gradle task / Scripts / Cargo binary 선택기
- **Aliases**: 자동 인식, bootRun, gradle task, script 선택 (inferred)
- **UI strings**: "Choose a folder to detect its default run command.", "Detected {}. Choose a script or target below.", "Detected {kind}. Default command available below.", "Detected {kind}, but no default run command. Select a task or enter a command.", "Gradle build file", "Gradle task", "Scripts · {}", "Cargo binary", "Select…", "Search name or command…", "No matching commands." (all untranslated)
- **Controls**: "Gradle task" 선택 → `gradle_commands`, "Scripts" 선택 → `node_commands`, "Cargo binary" 선택 → `cargo_commands`
- **Code path**:
  - UI: `src/ui/editor/form.rs` → `commands`, 선택기는 `src/ui/editor/picker.rs` → `show`
  - State: `src/ui/editor/detection.rs` → `refresh_detection`
  - 인식: `src/project.rs` → `detect_node_project`, `src/gradle.rs` → `detect_gradle_project`, `src/cargo_project.rs` → `detect`, `src/go_project.rs` → `detect`
- **Tests**: `src/ui/editor/detection_tests.rs`, `src/ui/editor/go_tests.rs`, `src/project.rs`, `src/gradle.rs`
- **Pitfalls**: Command에 붙인 `--no-daemon` 같은 옵션이 사라짐 → Gradle task를 고르면 Command를 `./gradlew <task>`로 덮어씀, source: `src/ui/editor/form.rs` → `gradle_commands`
- **Related**: `project-editor`

### `env-and-shell`: 환경 변수, .env, shell 설정과 실제 실행 명령 구성

- **Where**: 프로젝트 편집 창 → "Environment & shell"
- **Aliases**: 환경 변수, .env, nvm, PATH, 명령을 못 찾음 (inferred)
- **UI strings**: "Environment & shell", "Environment file (optional)", "Found in folder:", "In Browse, press ⌘⇧. to show hidden files.", "Shell (optional)", "Default login shell", "Environment variables", "Add variable" (all untranslated)
- **Controls**: "Add variable" → `advanced`, "Browse…" (env file) → `rfd` 파일 선택
- **Code path**:
  - UI: `src/ui/editor/form.rs` → `advanced`
  - 실행 명령: `src/process/command.rs` → `build_command`, `src/process/command.rs` → `shell_invocation`, `src/process/command.rs` → `merge_env`
- **Tests**: `src/process/command.rs`
- **Pitfalls**: nvm/sdkman 명령을 못 찾음 → 기본 `-lc`는 `.zshrc`를 읽지 않음, source: `src/process/command.rs` → `shell_invocation`; 파일 선택 창에 `.env`가 안 보임 → macOS 파일 창이 숨김 파일을 감춤, source: `src/ui/editor/form.rs` → `advanced`
- **Related**: `project-editor`, `server-lifecycle`

### `port-check`: 포트 사용 중 감지와 중복 포트 경고

- **Where**: 편집 창 Port 필드 힌트, 카드의 ⚠, 시작 실패 알림
- **Aliases**: 포트 충돌, port in use (inferred)
- **UI strings**: "already in use on this machine", "must be 1–65535", "also used by '{}'", "duplicate port", "Can't start '{}': port {port} is already in use." (all untranslated)
- **Controls**: none
- **Code path**:
  - 검사: `src/port.rs` → `is_port_free`, `src/port.rs` → `is_listening`, `src/port.rs` → `duplicate_config_ports`
  - 사용처: `src/main.rs` → `start_server`, `src/ui/editor.rs` → `port_hint`, 준비 감지는 `src/process/running.rs` → `probe_ready`
- **Tests**: `src/port.rs`, `src/process/running.rs`
- **Pitfalls**: Start 직후 0.0s 만에 Ready가 뜸 → bind 검사가 JVM의 `*:port`를 놓쳐 이전 앱을 새 서버로 착각, source: `51a89e3`; 프로세스가 Stopped인데 "port is already in use" → Gradle daemon과 앱이 별도 process group에서 아직 종료 중, source: `3e63be5`
- **Related**: `server-lifecycle`, `ready-crash-toast`

### `server-lifecycle`: 서버 시작, 중지, 재시작과 프로세스 트리 종료

- **Where**: pane 헤더 Run/Stop/Restart, 카드 우클릭 메뉴, pane 상태 텍스트
- **Aliases**: 시작, 중지, 재시작, 강제 종료, 안 꺼짐 (inferred)
- **UI strings**: "Run", "Stop", "Force stop", "Restart project", "Stopped", "Starting", "Running", "Stopping", "Crashed", "Failed to start '{}': {err}", "Not running. Use Run to start this command.", "Project exited. Run it again to collect new output." (all untranslated)
- **Controls**: "Run" → `Action::Start`, "Stop" / "Force stop" → `Action::Stop`, "Restart project" → `Action::Restart`
- **Code path**:
  - UI: `src/ui/workspaces/dock.rs` → `pane_header`
  - Handler: `src/main.rs` → `apply_action`, `src/main.rs` → `start_server`, 재시작 대기는 `src/main.rs` → `restart_pending`
  - 프로세스: `src/process/running.rs` → `poll`, `src/process/kill_tree.rs` → `tree_kill`, `src/process/command.rs` → `build_command`
  - Data: `running.json`, `src/main.rs` → `track_running`
- **Tests**: `src/process/running.rs`, `src/process/kill_tree.rs`
- **Pitfalls**: Stop을 눌러도 바로 안 꺼짐 → SIGINT 후 최대 10초 grace, 다시 누르면 SIGKILL, source: `src/main.rs` → `STOP_GRACE`; Restart가 "port is already in use"로 실패 → 이전 앱이 포트를 놓기 전에 재실행, source: `3e63be5`
- **Shares**: `src/process/running.rs`는 `log-view`와 `port-check`도 받침
- **Related**: `port-check`, `ready-crash-toast`, `session-recovery`

### `ready-crash-toast`: 준비 완료, 크래시, 오류를 하단 배너로 알림

- **Where**: 화면 하단 배너
- **Aliases**: Ready 알림, 크래시 알림, 토스트, 배너 (inferred)
- **UI strings**: "Ready", "'{}' is up ({secs:.1}s).", "Crashed", "'{}' exited{exit}." (all untranslated)
- **Controls**: none
- **Code path**:
  - 감지: `src/main.rs` → `ui`, `Starting → Running`과 `Crashed` 전이를 `notices`에 모음
  - UI: `src/main.rs` → `render_toast`, `src/ui/banner.rs` → `show`
- **Tests**: none
- **Pitfalls**: 알림 여러 개가 제목 없는 한 줄로 합쳐짐 → 배너 슬롯이 하나뿐이라 같은 frame의 notices를 join, source: `src/main.rs` → `notices`
- **Related**: `server-lifecycle`, `port-check`

### `delete-project`: 프로젝트 삭제와 확인 창

- **Where**: 카드 우클릭 "Delete" 또는 편집 창 "Delete project…" → 확인 모달
- **Aliases**: 삭제, 프로젝트 지우기 (inferred)
- **UI strings**: "Delete project?", "Remove '{name}' from Campfire? Its running process will stop and its saved configuration will be deleted. Files in the project folder will remain." (untranslated)
- **Controls**: "Delete" → `ConfirmOutcome::Confirm`, "Cancel" → `ConfirmOutcome::Cancel`
- **Code path**:
  - UI: `src/ui/confirm.rs` → `show_delete`
  - Handler: `src/main.rs` → `render_delete_confirm`, `src/main.rs` → `delete_server`
- **Tests**: none
- **Pitfalls**: none
- **Related**: `project-context-menu`, `project-editor`

### `workspace-tabs`: 로그 영역 상단의 워크스페이스 탭

- **Where**: 오른쪽 로그 영역 상단 탭 줄
- **Aliases**: 탭, 워크스페이스, 탭 이름 변경 (inferred)
- **UI strings**: "Workspace 1", "New workspace", "Workspace limit reached (100)", "Close workspace" (all untranslated)
- **Controls**: "New workspace" → `add`, "Close workspace" → `close`, 탭 더블클릭 → `rename_field`, 탭 가운데 클릭 → `strip`, Cmd/Ctrl+1–9, 0 / Cmd/Ctrl+W → `handle_shortcuts`
- **Code path**:
  - UI: `src/ui/workspaces/tabs.rs` → `strip`
  - State: `src/ui/workspaces/mod.rs` → `Workspaces`, 앱 재실행 시 저장하지 않음
- **Tests**: `src/ui/workspaces/mod.rs`
- **Pitfalls**: 탭 줄이 안 보임 → 비어 있는 워크스페이스가 하나뿐이면 일부러 숨김, source: `src/ui/workspaces/mod.rs` → `solo_hidden`
- **Related**: `log-dock`

### `log-dock`: 로그 pane 열기, 드래그 분할, pane 헤더

- **Where**: 오른쪽 로그 영역, 카드를 끌어 놓으면 반쪽 미리보기
- **Aliases**: 로그 창 분할, 로그 4개, 로그 창 닫기, 업타임 (inferred)
- **UI strings**: "Select a project to view its log. Use Open log or drag to compare up to 4 logs.", "Compact view · expand the window to compare logs", "Already open · release to focus", "A workspace shows at most 4 logs — close one first.", "Close log view", "Edit project" (all untranslated)
- **Controls**: 카드를 dock에 드롭 → `handle_card_drag`, "Open log" → `open_auto`, "Close log view" → `show_active` (`remove_recursively`), "Edit project" → `Action::OpenEdit`
- **Code path**:
  - UI: `src/ui/workspaces/dock.rs` → `show_active`, `src/ui/workspaces/dock.rs` → `pane_header`
  - 드롭: `src/ui/workspaces/drop.rs` → `zone_at`, `src/ui/workspaces/drop.rs` → `insert_at`
  - State: `src/ui/workspaces/mod.rs` → `open_auto`, `src/ui/workspaces/mod.rs` → `show_log`
- **Tests**: `src/ui/workspaces/drop.rs`, `src/ui/workspaces/dock.rs`, `src/ui/workspaces/mod.rs`
- **Pitfalls**: 다섯 번째 로그가 안 열림 → 워크스페이스당 pane 4개 상한, source: `src/ui/workspaces/mod.rs` → `FULL_NOTICE`
- **Shares**: `egui_tiles` 트리, 드래그 상태는 `project-reorder`와 같은 `SidebarDrag`
- **Related**: `workspace-tabs`, `log-view`

### `log-view`: 로그 본문 표시, follow, 줄바꿈, 지우기

- **Where**: 로그 pane 본문과 하단 툴바
- **Aliases**: 로그가 안 보임, 로그 사라짐, 자동 스크롤, 색상 (inferred)
- **UI strings**: "no output yet", "Clear output", "Follow new output", "Scroll to bottom", "Scroll to top", "Wrap long lines", "New lines · scroll to bottom and follow" (all untranslated)
- **Controls**: "Clear output" → `Action::ClearLogs`, "Follow new output" → `bottom_bar`, "Scroll to bottom" / "Scroll to top" → `bottom_bar`, "Wrap long lines" → `bottom_bar`, "↓ N" 배지 → `bottom_bar`
- **Code path**:
  - UI: `src/ui/log_view/mod.rs` → `show`, `src/ui/log_view/render.rs` → `render_body`, `src/ui/log_view/toolbar.rs` → `bottom_bar`
  - 색상: `src/ansi.rs` → `to_job`
  - Data: `src/process/log_buffer.rs` → `push`, `src/process/running.rs` → `poll`에서 채움
- **Tests**: `src/ui/log_view/render.rs`, `src/ui/log_view/mod.rs`, `src/process/log_buffer.rs`, `src/ansi.rs`
- **Pitfalls**: 맨 아래로 이동하면 pane이 비어 보임 → `f32::MAX` offset이 scroll bar가 숨으면 clamp되지 않음, source: `10e565a`; 오래된 로그가 사라짐 → 5 MiB ring buffer가 오래된 줄부터 버림, source: `src/process/log_buffer.rs` → `push`; 앱 재실행 후 복구된 서버의 로그가 비어 있음 → Adopted 프로세스는 pipe가 없음, source: `src/process/running.rs` → `Adopted`; 빨간 id clash 경고 → pane마다 다른 `salt` 없이 log view를 그림, source: `src/ui/log_view/mod.rs` → `salt`
- **Shares**: `src/process/log_buffer.rs`는 `log-search`도 읽음
- **Related**: `log-search`, `log-dock`

### `log-search`: 로그 찾기와 줄 필터

- **Where**: 로그 pane 하단 툴바 → 찾기 상자
- **Aliases**: 로그 검색, 찾기, 필터, 정규식 (inferred)
- **UI strings**: "Find in output (Cmd/Ctrl+F)", "Find in output", "Next match (Enter)", "Previous match (Shift+Enter)", "Match case", "Whole word", "Regular expression", "Filter lines", "Remove filter", "Invalid search expression", "Invalid filter · showing all lines", "{} matching lines", "no matching lines" (all untranslated)
- **Controls**: "Find in output (Cmd/Ctrl+F)" 툴바 버튼 → `bottom_bar`, Cmd/Ctrl+F → `show`, 찾기 상자 "Find in output" → `search_rows`, "Next match (Enter)" / "Previous match (Shift+Enter)" → `search_rows`, "Filter lines" → `search_rows`
- **Code path**:
  - UI: `src/ui/log_view/toolbar.rs` → `search_rows`
  - 매칭: `src/search.rs` → `is_match`, `src/search.rs` → `find_ranges`, 캐시는 `src/ui/log_view/cache.rs` → `collect_matches`
- **Tests**: `src/search.rs`, `src/ui/log_view/cache.rs`, `src/ui/log_view/mod.rs`
- **Pitfalls**: 필터를 켰는데 모든 줄이 보임 → 정규식이 잘못되면 필터를 끄고 전체를 보여 줌, source: `src/ui/log_view/toolbar.rs` → `search_rows`
- **Related**: `log-view`

### `resource-metrics`: 카드에 CPU와 메모리 표시

- **Where**: 실행 중인 프로젝트 카드 둘째 줄
- **Aliases**: CPU, 메모리, CPU 100% 넘음 (inferred)
- **UI strings**: "CPU {cpu:.0}%" (untranslated)
- **Controls**: none
- **Code path**:
  - UI: `src/ui/server_list.rs` → `metrics_row`
  - 수집: `src/metrics.rs` → `refresh`, `src/metrics.rs` → `subtree_usage`
- **Tests**: none
- **Pitfalls**: CPU가 100%를 넘음 → 프로세스 subtree 전체의 코어별 사용률을 합산, source: `src/metrics.rs` → `subtree_usage`
- **Related**: `project-sidebar`

### `session-recovery`: 중복 실행 방지와 이전 세션 서버 복구

- **Where**: 앱 실행 직후 다이얼로그와 배너
- **Aliases**: 이미 실행 중, 복구, 고아 프로세스 (inferred)
- **UI strings**: "Campfire is already running.", "Recovered {r} running project(s) from a previous session.", "Stopped {s} orphaned project(s) from a previous session." (all untranslated)
- **Controls**: none
- **Code path**:
  - 잠금: `src/process/instance_lock.rs`
  - 복구: `src/process/runtime_state.rs` → `confirmed_orphans`, `src/main.rs` → `reconcile_notice`
  - 종료 신호: `src/process/shutdown.rs`
  - Data: `running.json`, `campfire.lock`, written in `src/process/runtime_state.rs` → `save_to`, read in `src/process/runtime_state.rs` → `load_from`
- **Tests**: `src/process/runtime_state.rs`, `src/process/instance_lock.rs`, `src/process/shutdown.rs`
- **Pitfalls**: 두 번째 실행이 다이얼로그만 띄우고 꺼짐 → 살아 있는 첫 인스턴스의 lock을 발견, source: `src/process/instance_lock.rs`; 복구된 서버는 Stop은 되지만 로그가 없음 → Adopted는 PID만 보유, source: `src/process/running.rs` → `Adopted`
- **Related**: `server-lifecycle`, `config-storage`

### `config-storage`: 프로젝트 설정 저장과 로드

- **Where**: 화면에 없음, 저장 실패 시 배너
- **Aliases**: 설정 파일, servers.toml, 저장 실패 (inferred)
- **UI strings**: "Failed to save config: {err}" (untranslated)
- **Controls**: none
- **Code path**:
  - Data: OS 앱 설정 디렉터리의 com.heonny.campfire/servers.toml, written in `src/store.rs` → `save`, read in `src/store.rs` → `load`, 원자적 쓰기는 `src/fs_util.rs` → `write_atomic`
  - 모델: `src/model.rs` → `ServerConfig`
- **Tests**: `src/store.rs`, `src/fs_util.rs`, `src/model.rs`
- **Pitfalls**: none
- **Related**: `project-editor`, `project-reorder`

### `help-dialog`: 도움말과 버전 표시

- **Where**: 사이드바 헤더 (i) 아이콘 → 모달
- **Aliases**: 도움말, 버전, 단축키 (inferred)
- **UI strings**: "Help", "Campfire v", "Getting started", "Shortcuts", "Good to know", "Files", "Close" (all untranslated)
- **Controls**: "Help" (i 아이콘) → `Action::OpenHelp`, "Close" → `show`
- **Code path**:
  - UI: `src/ui/help.rs` → `show`
  - 버전: `build.rs`가 `CAMPFIRE_GIT_HASH`를 넣고 `CARGO_PKG_VERSION`과 함께 표시
- **Tests**: none
- **Pitfalls**: none
- **Related**: none
