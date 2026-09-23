<p align="center">
  <img src="assets/images/logo-full.png" width="300" alt="Campfire">
</p>

<p align="center">
  여러 로컬 개발 서버를 실행하고 관리하는 작고 가벼운 네이티브 데스크톱 앱.
</p>

<p align="center"><a href="README.md">English</a> · <b>한국어</b></p>

언어나 프레임워크에 상관없이 서버를 시작·중지·재시작하고, 로그를 실시간으로
보고, 포트 충돌을 잡아냅니다 — 모두 한 창에서. Rust와 egui로 만들어 별도 런타임
설치가 필요 없는 단일 경량 바이너리(~9 MB)입니다.

## 기능

- **모든 서버** — shell 명령으로 무엇이든 실행 (`npm run dev`,
  `./gradlew bootRun`, `go run .`, …)
- **프리셋** — Spring Boot, Flink, Next.js, Go, 또는 빈 Custom 항목. 명령과
  기본 포트를 미리 채워줍니다
- **폴더 자동 인식** — 폴더를 선택하면 이름과 기본 실행 명령을 채웁니다.
  Node의 `dev` / `start`와 npm·pnpm·Yarn·Bun, Gradle의 `bootRun` / `run`,
  Cargo binary, 루트에 main 패키지가 있는 Go 모듈(`go run .`)을 지원하며
  직접 수정한 명령과 기존 저장 설정은 보존합니다
- **서버별 설정** — 작업 디렉터리, 포트, 환경 변수, `.env` 파일, 선택적 shell
  오버라이드
- **생명주기** — 시작 / 중지 / 재시작. 프로세스 트리 전체를 종료(graceful
  `SIGINT` → 유예 시간 → `SIGKILL`)하여 고아 프로세스를 남기지 않습니다. 중지
  중에 다시 누르면 즉시 강제 종료됩니다
- **재정렬** — 프로젝트 카드를 위아래로 드래그해 순서를 바꿀 수 있고, 순서는
  저장됩니다
- **워크스페이스** — 카드를 로그 영역으로 드래그하면 로그가 열리고, 최대 4개를
  크기를 조절하며 탭으로 묶을 수 있습니다. Open log로 3~4개를 열면 2행으로
  배치하고, 작은 창에서는 기존 배치를 보존하며 로그를 전환합니다
- **준비/크래시 알림** — 포트가 열릴 때까지 *starting*으로 표시되고, 열리면
  준비 완료 배너가, 죽으면 exit code 배너가 뜹니다
- **브라우저로 열기** — 프로젝트의 `:port`를 클릭하면 `localhost:port`가 열립니다
- **실시간 로그** — ANSI 색상 렌더링에 검색, follow(tail), 줄바꿈 토글,
  안 본 줄 배지, 지우기까지. 상한이
  있는 5 MiB 링 버퍼 기반
- **포트 인식** — 이미 사용 중이거나 두 서버에 중복 할당된 포트를 경고하고,
  `PORT`와 `SERVER_PORT`를 모두 주입
- **리소스 사용량** — 각 프로젝트 카드에 서버별 CPU·메모리를 실시간 표시
  (프로세스 서브트리 전체를 합산)
- **크로스 플랫폼** — macOS와 Windows
- **로컬·프라이빗** — 모든 것이 내 컴퓨터에서 실행되고, 설정은 평범한 TOML 파일

## 다운로드

[Releases](https://github.com/heonny/campfire/releases/latest)에서 macOS Apple Silicon 또는 Windows x64 ZIP을 받으세요. 압축을 푼 뒤 macOS는 `bundle/Campfire.app`을 Applications로 옮기고, Windows는 `campfire.exe`를 실행하면 됩니다. Rust는 소스에서 빌드할 때만 필요합니다.

macOS 배포본은 macOS 11 이상이 필요합니다. 배포 인증서 서명·공증이 없어 첫 실행 시 OS가 차단하거나 경고할 수 있습니다. 각 릴리즈에 SHA-256 체크섬을 제공합니다.

## 요구 사항 (소스 빌드)

- [Rust](https://www.rust-lang.org/tools/install) (stable)
- macOS 또는 Windows

## 빌드 및 실행

사이드바, 대화상자, 알림에는 wgpu 렌더러 기반 `egui_glass`를 사용합니다.
시작 시 한 번 업로드한 정적 그라데이션을 배경으로 사용하며, 로그 패널은
가독성을 위해 불투명하게 유지합니다. wgpu 호환 그래픽 어댑터가 필요합니다.

```sh
# 개발 모드로 실행
cargo run

# 최적화된 릴리스 바이너리 빌드 (-> target/release/campfire)
cargo build --release
```

## 사용법

1. **+ Add**를 클릭하고 **작업 디렉터리**(Browse…)를 선택합니다.
2. 자동 입력된 이름과 명령을 확인합니다. 대상이 여럿이면 script / binary를
   선택하고, 탐지되지 않으면 명령을 직접 입력하거나 프리셋을 선택합니다.
3. 필요하면 포트, 환경 변수, `.env` 파일을 설정합니다.
4. 서버를 선택하고 **Start**를 누릅니다. 로그를 보면서 필요에 따라 **Stop**·
   **Restart** 합니다.

나머지 설명은 앱 안의 **Help** 버튼에 있습니다. 몇 가지 참고:

- **자동 인식** — 등록할 때 로컬 파일만 읽으며 의존성을 설치하거나 Gradle/Cargo를
  실행하지 않습니다. Gradle은 OS에 맞는 로컬 wrapper를 우선 사용하고 없으면
  PATH의 `gradle`을 사용합니다. Cargo는 `default-run`·명시적 binary·`autobins`를
  반영합니다. feature가 필요한 binary는 명령을 직접 입력하세요. workspace 멤버,
  Gradle 하위 모듈·플러그인 alias·사용자 정의 태스크는 재귀 탐색하지 않으므로 실행할
  패키지 폴더를 선택하거나 명령을 입력해야 합니다. 자동 인식은 포트를 설정하지
  않습니다. Go의 테스트 파일·build tag·OS/아키텍처 전용 진입점은 자동 선택하지
  않으며 `cmd/` 아래 실행 대상은 명령을 직접 입력해야 합니다. 실행에 필요한
  런타임과 의존성은 미리 준비되어 있어야 합니다.
- **포트** — 설정한 포트는 `PORT`(Node/Next)와 `SERVER_PORT`(Spring Boot)
  양쪽으로 주입됩니다. 프레임워크가 둘 다 읽지 않으면 명령에 포트를 직접 넣거나
  (예: `--server.port=8080`) 환경 변수로 지정하세요.
- **Shell / PATH** — 명령은 로그인 shell을 통해 실행됩니다. 버전 매니저(nvm,
  sdkman)가 잡히지 않으면 **Shell** 필드에 `zsh -lic`를 넣어 `~/.zshrc`를 읽게
  하세요.
- **설정** — 서버는 OS 앱 설정 디렉터리(`com.heonny.campfire/servers.toml`)에
  저장됩니다.

## 제거

Campfire는 두 개의 파일만 남깁니다 — 서버 목록(`servers.toml`)과 실행
상태(`running.json`). 앱을 지워도 이 파일들은 남으니, 깔끔하게 제거하려면 함께
지우세요.

| OS | 위치 |
|---|---|
| macOS | `~/Library/Application Support/com.heonny.campfire/` |
| Windows | `%APPDATA%\heonny\campfire\` 및 `%LOCALAPPDATA%\heonny\campfire\` |

함께 제공되는 스크립트가 대신 정리해 줍니다. 경로를 출력하고 먼저 확인을 받으며,
`--yes`(macOS) 또는 `-Yes`(Windows)를 주면 프롬프트를 건너뜁니다.

```sh
# macOS
./scripts/uninstall-macos.sh

# Windows (PowerShell)
.\scripts\uninstall-windows.ps1
```

그런 다음 앱 자체를 제거합니다: **Campfire.app**을 휴지통으로(macOS) 옮기거나
`campfire` 바이너리를 삭제(Windows)합니다. 스크립트는 앱은 건드리지 않고
데이터만 지웁니다.

## 사용 기술

Rust · [egui / eframe](https://github.com/emilk/egui) · egui_extras · command-group ·
sysinfo · rfd

## 라이선스

애플리케이션 코드는 MIT 라이선스를 따릅니다 — [LICENSE](LICENSE) 참고.

번들된 [Pretendard](https://github.com/orioncactus/pretendard) 폰트는 SIL Open
Font License(`assets/fonts/Pretendard-LICENSE.txt`)를 따릅니다.

[Lucide](https://lucide.dev) 아이콘은 ISC License(`assets/icons/LICENSE`)를
따릅니다.
