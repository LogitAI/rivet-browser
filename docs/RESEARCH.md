# RESEARCH

## 2026-10-25: Ember 브라우저 경계 조사

### 확인한 버전과 위치

- `darkpyonix-ember` 작업 트리의 `docs/design/REMOTE-BROWSER.md`와
  `ember/server/src/browser/`를 확인했다.
- 조사 시점은 2026-10-25다.

### 방법

- Ember의 원격 브라우저 설계 문서와 브라우저 API, instance, agent 모듈을 읽었다.

### 결과

- Ember는 현재 프로젝트마다 서버에서 headless Chromium 하나를 실행하고, 프로필을 서버 데이터 디렉터리에
  둔다.
- Ember는 선택한 컴퓨터의 SOCKS5 egress를 통해 그 headless Chromium의 요청을 보내며, egress 선택은
  프로젝트별로 저장한다.
- Ember는 CDP relay와 off-the-shelf DevTools MCP를 통해 Codex와 Claude Code에 브라우저를 제공한다.
- Ember 문서는 실제 Chrome과 실제 node의 end-to-end 확인, 원격 컴퓨터에서 MCP를 시작하는 위치를 아직
  미확인 항목으로 둔다.

### Rivet에 미치는 결정

- Rivet은 Ember의 서버 headless 브라우저를 대체한다고 가정하지 않는다.
- Rivet의 사용자 설정과 작업 공간 동기화는 Ember의 현재 브라우저 프로필 디렉터리와 분리한다.
- 원격 egress는 Ember의 컴퓨터 선택 모델을 재사용하되, Rivet의 직접 통합 브라우저를 어느 컴퓨터에서
  실행하고 제어할지는 PR-3에서 별도로 검증한다.

### 남은 조사

- Chrome app window와 Rivet 런타임을 지원 브라우저 버전별로 직접 통합하는 실험.
- Windows Chrome 프로필을 복사하지 않고 Ember 전환 경험을 유지하는 동기화 범위.
- Ember의 CDP relay와 Rivet의 로컬 제어 채널을 결합할 때의 인증과 권한 모델.

## 2026-10-28: Chrome 일반 창의 타이틀 배치 조사

### 확인한 환경

- Windows 빌드 `10.0.26300.0`과 Google Chrome `156.0.8078.48`을 조사했다.
- 설치된 `chrome.dll`의 공개 export는 다섯 개이며, 프레임 배치 함수는 포함하지 않는다.
- `chrome.dll`의 CodeView 정보는 GUID `{7B131951-33A2-9E0F-4C4C-44205044422E}`와 age `1`이다.
  Google 심볼 서버에서 이 식별자로 받은 PDB의 요약은 같은 GUID와 age `2`를 보고한다.
  이 차이가 실제 디버거의 심볼 일치 판정에 미치는 영향은 아직 확인하지 않았다.
- PDB는 작업 트리의 `.scratch/chrome-symbols/156.0.8078.48/`에만 두었다. 설치본에는 넣지 않는다.
  서버 저장 크기는 약 1.08GB이며 내려받은 크기는 5,709,946,880바이트다.

### 확인한 내용

- Chrome의 Windows 프레임에서 `LayoutClientView`는 `GetTopInset(false)`를 이용해 웹 콘텐츠 영역을
  계산한다. `GetTopInset`은 탭이 없는 창에서 `TitlebarHeight`를 사용하고, 이 값은 전체화면일 때만
  0이다. Windows의 `WS_CAPTION` 제거만으로는 이 계산이 없어지지 않는다.
  [Chromium 프레임 구현](https://chromium.googlesource.com/chromium/src.git/+/refs/heads/main/chrome/browser/ui/views/frame/browser_frame_view_win.cc)
- `--start-fullscreen`은 콘텐츠 영역을 늘리지만 Chrome의 전체화면 종료 UI를 남긴다. 따라서 현재 데모는
  일반 창에서 자체 타이틀 영역을 제거한 구현으로 볼 수 없다.
- PDB 공개 심볼에서 `BrowserFrameViewWin::GetTopInset`, `GetBoundsForClientView`,
  `GetBrowserLayoutParams`, `Layout`, `views::HWNDForView`를 찾았다. 주소는 해당 빌드의 결과일 뿐이며
  다른 Chrome 버전에 그대로 적용할 수 없다.
- 같은 PDB에서 `DesktopWindowTreeHostWin::GetContentWindowForHWND`,
  `views::Widget::GetWidgetForNativeWindow`, `BrowserWidget::GetFrameView`도 찾았다. 함수 이름과 선언을
  기준으로 하면 HWND에서 프레임 객체를 찾고 `HWNDForView`로 역확인하는 경로를 실험할 수 있다.
  아직 실행하지 않았으므로 반환 객체의 정확한 타입, 스레드 조건, 실패 동작은 검증되지 않았다.
- `Layout`의 x64 기계어는 여러 배치 경로를 인라인한다. `GetTopInset` 심볼 하나만 변경하면 모든 레이아웃이
  바뀐다고 가정할 수 없다. 콘텐츠 크기, 페인팅, 입력 위치를 함께 검증해야 한다.
- 현재 회귀 검사 `fr2_browser_frame_does_not_require_fullscreen`은 의도적으로 건너뛰며, 명시 실행 시
  실패한다. 기존 좌표 검사 통과는 이 요구를 입증하지 않는다.

### 남은 실험

- 특정 Rivet 창의 Chrome 프레임 객체를 다른 사용자 창과 구분할 수 있는지 확인한다.
- 일반 창 상태에서 해당 객체의 클라이언트 영역 배치와 입력 처리가 함께 바뀌는지 검증한다.
- 심볼 식별과 바이너리 확인에 실패한 브라우저 버전은 변경하지 않고 오류를 반환한다.
- 일반 Chrome 창과 Rivet 창을 동시에 열어 격리, IME, 권한 창, 다운로드, DPI 변경을 확인한다.

## 2026-10-28: 브라우저 레이아웃 훅과 합성 사이드바 실험

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 주 모니터 200%(192 DPI), 보조 모니터 125%(120 DPI).
- Chrome은 `--app`으로 열었다. 측정은 `scripts/tests/windows-frame.ps1`과 `PrintWindow` 캡처로 했다.

### 이전 구현에서 확인한 문제

- 런타임이 `RivetHostWindow`라는 별도 최상위 창을 만들고 Chrome 최상위 창을 `WS_CHILD`로 바꿔 그 안에
  넣었다. Chrome은 캡션 버튼을 클라이언트 영역에 직접 그리므로 버튼이 두 벌 보였다. 이를 감추려고
  `--start-fullscreen`을 썼고, 별도 스레드가 16ms마다 메시지를 확인했다.

### 그리기 경로 실험

- Chrome 최상위 창의 확장 스타일은 `0x200100`이다. `WS_EX_NOREDIRECTIONBITMAP`이 있어 GDI로 그린
  non-client 영역은 화면에 나오지 않았다.
- 같은 창에 DirectComposition 타깃을 만들 수 있었다. 브라우저 프로세스 안에서 만들어야 한다.
- DirectComposition visual의 원점은 클라이언트 영역 왼쪽 위다. 클라이언트 밖으로 놓은 부분은 정확히 경계에서
  잘렸다. 따라서 non-client 영역을 예약하는 방식으로는 같은 표면에 그릴 수 없다.

### 레이아웃 함수

- PDB 공개 심볼에서 `BrowserFrameViewWin`의 주 vtable(RVA `0x0FE73E28`)을 찾았다.
  `GetBoundsForClientView`는 103번 칸이고 `this+0x378`의 캐시된 사각형을 돌려준다.
  `GetBrowserLayoutParams`(124번 칸)도 같은 사각형을 첫 필드로 쓴다.
- 103번 칸을 바꿔 사각형을 오른쪽으로 292 DIP 옮기자 Chrome이 타이틀바와 콘텐츠를 그 옆에 배치했다.
  200%에서 렌더러 창은 클라이언트 왼쪽에서 584px 떨어져 시작했다.
- 실제 마우스 입력으로 링크를 눌러 이동을 확인했다. 좌표 보정은 필요 없었다.
- vtable은 같은 프로세스의 모든 Chrome 창에 공유된다. 훅은 `views::HWNDForView`로 창을 찾아 Rivet 창만
  바꾼다.
- 공개 PDB의 `BrowserFrameViewWin` 타입에는 멤버 목록이 없었다. 창 제목 뷰의 위치를 바꾸려면 다른 방법이
  필요하다.
- `chrome-builds` 도구는 PDB와 바이너리만으로 손으로 찾은 값과 같은 표 항목을 만들었다. Google 심볼 서버의
  경로는 `chrome.dll.pdb/<GUID><age>/chrome.dll.pdb`이다.

### 동작 확인

- 최대화, 복원, 최소화 후 복원에서 프레임 검사가 통과했다.
- F11 전체화면에서 사이드바가 빠지고 해제하면 돌아왔다.
- 보조 모니터로 옮기면 사이드바가 365px(292 DIP, 125%)이 되고 검사가 통과했다. 모니터를 넘는 동안에는
  Windows DPI와 Chrome 배율이 잠시 달랐다. 사이드바는 Chrome이 실제로 배치한 배율을 따르게 했다.
- 사이드바가 창 위쪽 테두리까지 덮어 Chrome이 조금 아래에서 시작하는 것처럼 보였다. 사이드바를
  `DWMWA_VISIBLE_FRAME_BORDER_THICKNESS`만큼 내려 테두리가 창 전체 폭에 이어지게 했다.
- 125%에서는 Chrome 렌더러 창이 반올림으로 1px 넓게 잡혔다. 검사는 1px 차이를 허용한다.
- 가려진 창은 Chrome이 렌더러를 숨긴다. 백그라운드 확인은 `--disable-features=CalculateNativeWinOcclusion`으로
  먼저 연 같은 프로필 프로세스에서 했다.

### 작업 표시줄 아이콘

- Chrome app window는 Chrome의 AppUserModelID를 써서 Chrome 아이콘으로 묶였다.
- `WM_SETICON`의 단일 크기 아이콘만 있으면 작업 표시줄이 크기를 바꿔 흐리게 그렸다.
  `RelaunchIconResource`가 여러 크기의 `rivet.ico`를 가리키면 선명했다.

### 남은 확인

- 최소화와 최대화 애니메이션의 육안 확인, 한국어 IME, 권한 창, 다운로드, 드래그앤드롭.
- 첫 정기 CI 실행이 GitHub Windows 실행기에서 화면 검사를 통과하는지.

## 2026-10-28: 패널 상단 줄을 Chrome 세로 탭 모드로 만드는 조사

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`. 실험은 `.scratch/vt-profile`의 별도 프로필에서만 했다.

### 결과

- Chrome 156에는 세로 탭이 제품 기능으로 들어 있다. `VerticalTabsLaunch` 기능, 프로필 설정
  `vertical_tabs.enabled`, `vertical_tabs.uncollapsed_width`, `vertical_tabs.collapsed_state`, 창 시스템 메뉴의
  전환 항목(`kToggleVerticalTabsElementId`)을 바이너리에서 확인했다.
- 별도 프로필에 `vertical_tabs.enabled`를 켜고 일반 창을 열자, 맨 위 한 줄에 뒤로, 앞으로, 새로고침,
  주소창, 프로필, 메뉴와 Chrome 캡션 버튼이 모이고 왼쪽에 세로 탭 띠가 생겼다. 목업의 "Appbar와 맨 위 줄이
  주소 줄인 페이지" 구성과 같다. `VerticalTabs.Toolbar.CaptionAreaPressed` 기록 이름으로 보아 이 줄은 창의
  캡션 역할도 한다.
- 이 설정은 프로필 단위라서 켜면 같은 프로필의 일반 Chrome 창도 모두 세로 탭이 된다. Rivet 창에만 적용해야
  한다.
- 창마다 하나씩 있는 `tabs::VerticalTabStripStateControllerImpl`에서 판정 함수를 찾았다.
  - `ShouldDisplayVerticalTabs`(RVA `0x0369C6E0`)는 `this+0x1E8`의 한 바이트를 돌려주는 두 명령어 함수다.
  - `GetUncollapsedWidth`(RVA `0x0369C700`)는 `this+0x1CC`의 폭을 돌려준다.
  - `ShouldDisplayVerticalTabs`를 가리키는 vtable 칸은 RVA `0xFDAFE00` 하나뿐이다.
- 따라서 현재 레이아웃 훅과 같은 방식으로 이 칸을 바꾸면 Rivet 창에서만 세로 탭 배치를 쓸 수 있다.

### 남은 조사

- 이 컨트롤러가 어느 창의 것인지 알아내는 방법. 컨트롤러에서 Browser, 창, HWND로 가는 경로를 찾아야 한다.
- 세로 탭 띠 자리를 Appbar로 덮을 때 입력을 Rivet과 Chrome 중 어디로 보낼지.
- 세로 탭 띠의 폭을 Appbar 폭(268 DIP)에 맞추는 방법. `GetUncollapsedWidth`나 설정 값으로 가능해 보인다.

## 2026-10-28: Rivet 창에만 Chrome 세로 탭을 켜는 실험

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터. 별도 프로필 `.scratch/vt-rivet`에서만 했다.

### 결과

- `--app` 창에는 탭 띠와 도구 모음이 없어서 세로 탭을 켤 수 없다. 실험은 Rivet이 일반 창을 열도록 바꾼 빌드로 했다.
  일반 창에서도 Appbar와 레이아웃 훅은 그대로 동작했다.
- `ShouldDisplayVerticalTabs`는 창이 붙은 뒤에도 레이아웃마다 불린다. 하지만 실제 모드 값(`this+0x1E8`)이 가로인데
  `true`로 답하면 Chrome이 바로 종료된다. 판정만 바꾸는 방법은 쓸 수 없다.
- 모드 값을 1로 쓰고 `NotifyModeChanged`와 `UpdateCollapseActionItem`을 부르면, 설정을 바꿀 때와 같이 그 창만 세로 탭
  배치로 다시 만들어진다. 레이아웃 도중이 아니라 창 메시지로 미뤄서 불렀다.
- 그 결과 맨 위 한 줄에 Chrome의 뒤로, 앞으로, 새로고침, 주소창, 확장, 메뉴와 Windows 캡션 버튼이 모이고 그 아래에 웹
  페이지가 놓였다. 세로 탭 띠(240 DIP)는 Appbar 밑에 두고 레이아웃 훅으로 나머지 28 DIP만 밀었다.
- 같은 프로필에서 새로 연 일반 창은 가로 탭 그대로였다. 프로필 설정에 `vertical_tabs.enabled`가 기록되지 않았다.
- `this+0x1CC`의 폭 값을 직접 바꾸면 효과가 없었다. 탭 띠 폭은 `vertical_tabs.uncollapsed_width` 설정에서 온다.
- 컨트롤러에서 창을 찾는 경로는 아직 틀렸다. `this+0x1B8`을 Browser로 보고 `GetBrowserViewForBrowser`와
  `HWNDForView`를 부르면 Chrome이 종료된다. 이번 실험은 Rivet 창이 하나라는 가정으로 첫 컨트롤러만 바꿨다.

### 남은 조사

- 컨트롤러에서 HWND로 가는 올바른 경로.
- 세로 탭 띠 폭이 사용자 설정이나 창 크기에 따라 바뀔 때 Appbar와 맞추는 방법.
- Rivet이 `--app` 대신 일반 창을 여는 결정. INTENT D2와 SPEC FR-2를 바꿔야 한다.

## 2026-10-28: 세로 탭 창 식별과 가로 탭바 숨기기 비교

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, 별도 프로필 `.scratch/vt-rivet`.

### 세로 탭: 컨트롤러에서 창 찾기

- `this+0x1B8`은 `Browser`이고(`Browser` vtable `0xFE58AB0`), `GetBrowserViewForBrowser`는 `BrowserView`를
  돌려준다(vtable `0xFDF8A08`). 이 포인터는 `views::View` 기반이 아니라서 `HWNDForView`에 그대로 넘기면 Chrome이
  종료된다.
- `BrowserView`의 `views::View` vtable은 `0xFE7BF68`이다(`+0x58` 칸이 `View::GetWidget`). 객체 안에서 이 vtable을
  찾으면 `+0x1F0`에 있고, 그 주소를 넘기면 창 핸들이 맞게 나온다.
- 판정 함수 안에서는 컨트롤러만 기록하고, 각 Rivet 창이 메시지를 받은 뒤 컨트롤러의 창이 자기인지 확인하고 전환했다.
  Rivet 창은 세로 탭이 되고, 같은 프로세스의 일반 창은 "다른 창"으로 판정되어 가로 탭 그대로였다.
- 같은 프로필로 `rivet session start`를 두 번 하면 두 번째는 기존 창의 새 탭으로 열리고 CLI가 창을 찾지 못한다.
  여러 Rivet 창은 아직 확인하지 못했다.

### 가로 탭바 숨기기

- 탭바 표시를 정하는 `BrowserView::ShouldDrawTabStrip`(`0x5286C00`)과 `GetTabStripVisible`(`0x57F5EC0`)은 가상
  함수가 아니다. vtable 칸 교체로는 바꿀 수 없고, 함수 코드를 직접 덮어써야 한다.
- 실험으로 `ShouldDrawTabStrip`을 프로세스 전체에서 `false`로 덮어쓰자 탭바는 사라졌지만, 캡션 버튼만 있는 빈 제목
  줄이 남고 도구 모음은 두 번째 줄에 놓였다. 팝업 창과 같은 두 줄 배치다.
- 도구 모음을 캡션 버튼 줄로 올리려면 레이아웃을 더 바꿔야 한다. 후보는 프레임 뷰의 가상 함수
  `GetBrowserLayoutParams`(`0x369E0E0`)이지만 구조를 아직 모른다.

### 비교

- 세로 탭 방식은 Chrome이 한 줄 배치를 직접 만들고, Rivet은 vtable 칸 하나와 Chrome 내부 함수 호출 두 개만 쓴다.
- 탭바 숨기기 방식은 함수 코드 덮어쓰기와 추가 레이아웃 변경이 필요하고, 지금 상태로는 두 줄 배치가 된다.

## 2026-10-28: Rivet이 패널 상단 줄을 직접 그리는 실험

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창, 별도 프로필 `.scratch/vt-rivet`.

### 결과

- Appbar와 같은 DirectComposition 대상에 두 번째 비주얼을 붙여, Appbar 오른쪽부터 캡션 버튼 왼쪽까지의 제목 줄에
  뒤로, 앞으로, 새로고침, 보안 표시, 제목을 그렸다. Chrome의 제목 글자는 이 비주얼에 가려진다.
- 버튼 자리는 `WM_NCHITTEST`에서 Rivet 영역으로, 나머지는 `HTCAPTION`으로 돌려 창 끌기를 유지했다.
- 버튼은 Windows 표준 `WM_APPCOMMAND`(`APPCOMMAND_BROWSER_BACKWARD`, `FORWARD`, `REFRESH`)를 창에 보낸다. 링크로 이동한
  뒤 뒤로를 누르면 이전 페이지로, 앞으로를 누르면 다시 다음 페이지로 갔다. Chrome 내부 함수는 쓰지 않았다.
- 창 제목이 바뀔 때(`WM_SETTEXT`) 다시 그려 제목이 따라 바뀌었다.

### 목업과 남은 차이

- 캡션 버튼 영역은 Chrome의 제목 줄 색이라 Rivet 줄과 색이 다르다. 프레임 뷰의 가상 함수
  `BrowserFrameViewWin::GetTitlebarColor`(`0xA403B30`)가 후보다.
- 줄 높이가 Chrome 제목 줄(32 DIP)이다. 목업은 44 DIP다.
- 둥근 패널, 패널 둘레 여백, 그림자가 없다.
- 주소 대신 페이지 제목만 보인다. URL 읽기, 주소 입력, 자동완성, 뒤로/앞으로 가능 여부 표시는 아직 없다.

## 2026-10-28: 캡션 버튼 영역 색 맞추기 시도

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창.

### 결과

- `BrowserFrameView::GetFrameColor`(`0x3121500`)는 프레임 뷰 vtable 133번과 281번 칸에 있다. 두 칸을 바꾸고,
  이 함수를 직접 부르는 `BrowserFrameViewWin::GetTitlebarColor`(`0xA403B30`, 7바이트와 패딩)의 시작도 Rivet으로
  가는 점프로 바꿨다. 설치는 성공했지만 창을 다시 그려도 세 경로 모두 한 번도 불리지 않았다.
- 창은 Mica(`DWMWA_SYSTEMBACKDROP_TYPE` 2)를 쓰지만, Mica를 끄거나 `DWMWA_CAPTION_COLOR`를 바꿔도 화면 캡처의 캡션
  영역 색은 그대로였다. 이 색은 Windows가 아니라 Chrome이 칠한다.
- Chrome 156의 앱 창 제목 줄은 프레임 색 함수가 아니라 색 공급자(`ui::ColorProvider`)의 프레임 색을 바로 쓰는 것으로
  보인다. 다음 후보는 이 창의 색 공급자 또는 색 믹서다.

## 2026-10-28: Rivet 창에만 캡션 영역 색 적용

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창, 별도 프로필 `.scratch/vt-rivet`.

### 결과

- `BrowserFrameViewWin::OnPaint`(`0x11B5610`, 프레임 뷰 vtable 89번 칸)는 `views::View::GetColorProvider`
  (`0x4AA27D0`)로 색 공급자를 얻고, `ui::ColorProvider::GetColor`(`0x4AA24F0`)로 활성 `0x13C`, 비활성 `0x141` 색을
  읽어 제목 줄을 칠한다. 색 공급자는 `+0x18`의 트리에 색을 두며 노드의 `+0x20`이 색 ID, `+0x24`가 색이다.
- 이 값을 그대로 바꾸면 같은 프로필의 일반 창 탭 줄도 바뀌었다(`#d3e3fd`에서 `#fbfaf7`). 색 공급자를 창끼리 공유한다.
- `OnPaint` 칸을 감싸 Rivet 창이 그리는 동안에만 두 색을 바꾸고 그린 뒤 되돌리자, Rivet 창은 활성과 비활성 모두 캡션
  영역이 `#fbfaf7`이 되었고 같은 프로필의 일반 창은 `#d3e3fd` 그대로였다. 캡션 버튼은 Windows 기본 모양 그대로다.
- `GetFrameColor` vtable 칸과 `GetTitlebarColor`를 바꾸는 방법은 그리기에 쓰이지 않아 효과가 없었다.

## 2026-10-28: 둥근 페이지 패널과 44 DIP 상단 줄

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창.

### 결과

- 프레임 뷰 vtable 127번 칸 `BrowserFrameViewWin::GetTopInset`(`0xA4033E0`)에 12 DIP를 더하자 Chrome이 페이지를
  44 DIP 아래에서 시작했다. 레이아웃 훅은 오른쪽과 아래에 8 DIP 여백만 뺀다.
- 세 번째 DirectComposition 비주얼에 두께가 여백의 두 배인 둥근 사각형 선을 패널 바깥 한 여백 위치에 그려, 여백을
  프레임 색으로 칠하면서 패널 모양(반지름 12 DIP)만 비웠다. Appbar와 캡션 버튼 위는 잘라 내고 칠하지 않았다.
  1 DIP 테두리 선도 함께 그렸다.
- 캡션 버튼은 32 DIP 높이 그대로 위에 붙어 있다. 버튼 선호 크기(`WindowsCaptionButton::CalculatePreferredSize`,
  vtable `0xFF025C8` 74번 칸)를 바꿔도 효과가 없었다. `BrowserFrameViewWin::Layout`은 캡션 컨테이너 높이를
  `BrowserView::GetFrameElementInfo`(`0x824EF0`)에서 계산하며, 이 경로는 가상 함수가 아니다.

## 2026-10-28: 상단 줄의 페이지 주소

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창.

### 결과

- Chrome은 접근성 트리의 문서(Document) 요소 값으로 페이지 주소를 내놓는다. 바깥 프로세스에서 UI Automation으로
  읽었을 때 `https://example.com/`이 나왔고 42 ms가 걸렸다.
- 런타임이 작업 스레드에서 같은 방법으로 주소를 읽고, 창에 메시지를 보내 상단 줄을 다시 그리게 했다. 제목이 바뀔 때마다
  다시 읽는다. 상단 줄에 `example.com`이 표시되었다.
- Chrome 내부 주소나 함수 위치에 기대지 않으므로 Chrome 버전이 바뀌어도 그대로 동작할 가능성이 높다.
- 대신 UI Automation 클라이언트가 붙으면 Chrome이 접근성 트리를 계속 만든다. 이 비용은 아직 재지 않았다.
- 제목이 바뀌지 않는 이동(같은 문서 안의 해시 이동 등)은 아직 따라가지 못한다.

## 2026-10-28: 상단 줄의 주소 입력, 이동 상태, 44 DIP 캡션 버튼

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, `--app` 창.

### 결과

- 창에서 활성 페이지까지는 `views::DesktopWindowTreeHostWin::GetContentWindowForHWND`(`0xD564A0`),
  `BrowserView::GetBrowserViewForNativeWindow`(`0xBC81E10`), `BrowserView::GetActiveWebContents`(`0xBC82340`)로
  닿는다. 모두 UI 스레드에서 부른다.
- 뒤로 가능 여부는 `chrome::CanGoBack(WebContents*)`(`0xA0B7C00`), 앞으로 가능 여부는 `WebContents` vtable 6번 칸
  `GetController`로 얻은 컨트롤러에 `NavigationControllerImpl::CanGoForward`(`0x82F3C50`)를 불러 얻었다. 첫 페이지에서는
  뒤로가 흐리게, 이동 뒤에는 진하게, 뒤로 간 뒤에는 앞으로가 진하게 그려졌다.
- 표시 주소는 `WebContentsImpl::GetVisibleURL`(`0x5CF8BA0`)에서 바로 읽었다. GURL의 첫 필드는 libc++ 문자열이며
  Chromium 배치에서는 마지막 바이트의 최상위 비트가 긴 형식을 뜻한다. 짧은 형식은 글자가 제자리에 있고 길이는 마지막
  바이트에 있다. 접근성 트리로 읽은 주소는 제목이 바뀐 직후 이전 오류 페이지 주소로 남는 경우가 있어 대체 경로로만 남겼다.
  제목이 바뀌지 않는 이동을 위해 상단 줄은 0.5초마다 다시 확인하고, 바뀐 것이 없으면 그리지 않는다.
- 주소 입력은 `GURL(std::string_view)`(`0x42BC7A0`), `content::OpenURLParams` 생성자(`0x72E8660`),
  `WebContentsImpl::OpenURL`(`0x366BC50`)로 현재 탭에서 연다. 전이는 `TYPED | FROM_ADDRESS_BAR`다. `iana.org`와
  `www.iana.org/domains/root`를 입력해 해당 페이지로 이동했고 상단 줄 주소도 바뀌었다. 점이 없거나 공백이 있는 입력은
  검색 주소로 바꾼다. GURL 크기는 기본 생성자에서 `0x78`로 읽었다.
- 상단 줄은 키보드 입력을 받을 수 없어 주소를 누르면 소유 팝업 EDIT 창을 그 위에 연다. 클릭 처리 중에 열면 Chrome이 곧바로
  포커스를 되가져가 닫혔다. 클릭이 끝난 뒤 메시지로 열고, 포커스가 브라우저 창 자신으로 넘어갈 때만 되찾게 하자 유지되었다.
  입력하는 동안 브라우저 창은 비활성 상태로 그려진다(캡션 버튼이 흐려짐).
- 캡션 버튼 컨테이너는 `BrowserFrameViewWin::Layout`(프레임 뷰 vtable 23번 칸)이 탭 줄 정보로 크기를 정한다. 이 칸을 감싸
  원래 레이아웃 뒤 Rivet 창에서만 컨테이너(`+0x3C8`) 높이를 44 DIP까지 `views::View::SetBoundsRect`(`0x4F42540`)로 다시
  정하자, 닫기 버튼 hover 영역이 상단 줄 전체 높이가 되고 기호가 세로 가운데에 왔다. 뷰 경계는 `+0x1C8`에 있다.
- 남은 차이: 최소화·최대화 버튼 아래 32~44 DIP 구간에 페이지 배경색 띠가 보인다. Rivet 비주얼은 버튼 위에 그려지므로 그
  구간을 덮으면 버튼 hover 아래쪽도 가려진다.

## 2026-10-28: Chrome 툴바를 덮어써서 상단 줄로 쓰기

### 확인한 환경

- Windows `10.0.26300`, Google Chrome `156.0.8078.48`, 192 DPI 모니터, 일반 창, Rivet 세로 탭 실험 켬.

### 결과

- 일반 창을 Chrome 세로 탭 모드로 바꾸면 뒤로, 앞으로, 새로 고침, 주소창, 캡션 버튼이 한 줄에 놓인다. 주소창에 `iana`를
  입력하자 인라인 자동완성, 방문 기록 제안, 한국어 검색 제안이 모두 Chrome 것으로 동작했다.
- `GetLayoutConstant`(`0x4713020`)는 가상 함수가 아니어서 앞 16바이트를 Rivet 함수로 가는 절대 점프로 덮어썼다.
  원래 함수는 트램펄린으로 부른다. 앞 9바이트는 그대로 옮기고, 10번째 명령은 자기 위치 기준으로 스택 쿠키를 읽으므로
  같은 주소를 절대 주소로 읽게 바꿨다. 돌아가는 점프가 `rax`를 쓰면 쿠키 값을 덮어써 Chrome이 죽었고, `jmp [rip]`으로
  바꾸자 정상 동작했다.
- PDB의 `LayoutConstant` 열거형은 0부터 순서대로이며 75개다. 45번 `kTabstripToolbarOverlap`이 프레임 레이아웃의 호출과
  맞는다. 주소창 높이(16), 툴바 버튼 높이(46), 아이콘 크기(47), 주소창 모서리(12), 페이지 영역 모서리(29)를 바꾸자,
  훅 설치 뒤 만든 창의 툴바가 낮아지고 주소창 모서리가 덜 둥글어졌다. 훅 설치 전에 만든 창은 `views::Widget::ThemeChanged`
  (`0x1DC5870`)를 불러도 주소창 높이가 그대로였다. 런타임이 창보다 늦게 들어오므로 생성 시점 값을 바꾸는 방법이 더 필요하다.
- `BrowserAppMenuButton`의 View vtable(`0xFEA63C8`) 99번 칸 `OnThemeChanged`를 감싸 `views::View::SetVisible(false)`
  (`0x5735770`)를 부르고 `Widget::ThemeChanged`로 다시 돌리자 메뉴(⋮) 버튼이 사라졌다. 메뉴 버튼에 붙는 말풍선은 기준점을
  잃고 창 밖으로 밀렸다.
- 뒤로 버튼 아이콘은 `vector_icons::kBackArrowChromeRefreshOldIcon`(`0x10E02EA8`)이다. `gfx::VectorIcon`의 첫 필드인
  표현 목록 포인터를 Rivet 경로로 바꾸자 버튼 모양이 바뀌었다. `gfx::CommandType` 값은 PDB 열거형 순서와 같다
  (`MOVE_TO` 7, `LINE_TO` 11, `CIRCLE` 24, `CANVAS_DIMENSIONS` 28 등).
- 세 변경 모두 프로세스 전체에 적용된다. 같은 프로세스의 일반 Chrome 창도 바뀐다.

## 2026-10-29 ~ 2026-11-02: Chromium 계열 다른 브라우저의 행

- 대상: Microsoft Edge `156.0.4314.50`, `.58`, Brave `156.1.98.62`, `.63`, Naver Whale `5.39.412.57`(Chromium
  `150.0.7871.252`), Samsung Internet `30.0.4.29`(Chromium `143.0.7499.194`). Windows `10.0.26300`, 화면 밖 시험
  프로필(동기화와 로그인 끔).
- 행에는 각 이진 안의 Chromium 함수만 기록한다. Chrome PDB의 이름과 공개 심볼, 호출 그래프, 추적 이벤트 문자열,
  vtable 이웃으로 같은 Chromium 함수를 찾고 앞부분 바이트를 설치된 DLL에서 확인했다.
- 공개 PDB가 있는 빌드(Edge, Brave)는 Chrome과 같은 방식으로 이름에서 행을 만든다. 공개 PDB가 없는 빌드는 Chrome
  같은 마일스톤 PDB에서 출발해 찾았고, 확신도는 정답과 대조하지 못한 값이다.
- 같은 Chromium 클래스라도 멤버 오프셋, vtable 슬롯, 색 ID, `gfx::CommandType` 값이 빌드마다 다를 수 있다. 행이 그
  값을 빌드마다 따로 든다.
- 같은 마일스톤 안의 패치 빌드 사이(Brave `.62`에서 `.63`, Edge `.50`에서 `.58`)는 `chrome-builds port`가 주소를 모두
  옮겼고 검토 항목이 0개였다.
- Chromium의 분할 탭 코드(`TabStripModel::AddToNewSplit`, `RemoveSplit`, `GetSplitForTab`, `MultiContentsView`)는
  모든 대상 이진에 들어 있다. 마일스톤 150 이하의 `AddToNewSplit`은 활성 탭이 아닌 인덱스 하나만 받고, 인덱스 벡터가
  begin, end, capacity 포인터이다(156은 begin, 크기, capacity). 행의 `split_adds_active`와
  `split_vector_ends_by_pointer`가 이 차이를 나타낸다.
- 모든 대상에서 Chromium 경로만으로 화면 기능이 같게 나오지는 않았다. 브라우저가 Chromium의 창 배치를 그대로 쓰지
  않는 경우, 탭 모델의 분할이나 `SidePanelCoordinator`가 화면에 나오지 않았다. 이 결과가 Chromium 코드만 쓰고 app
  window와 Rivet의 두 번째 `ContentsWebView`로 분할과 Sidebar를 만드는 결정으로 이어졌다(2026-11-04 기록).
- 화면 밖 창에서는 게시한 `WM_MOUSEMOVE`로 캡션 hover가 그려지지 않고 `WM_NCMOUSEMOVE`(`HTCLOSE` 등)로 그려진다.
  `PrintWindow` 캡처의 페이지 영역은 Rivet 없는 창에서도 비어 보일 수 있다. 캡처의 한계다.
- 개발자 모드 단추의 브라우저 명령 번호는 가속키 표에서 읽는다. Ctrl+Shift+C `40023`, Ctrl+Shift+J `40005`,
  Ctrl+Shift+I `40004`, F12 `40237`이고 Chrome, Edge, Brave에서 같다. 창에 `WM_COMMAND`로 보내면 단축키와 같은 일을
  한다. 네트워크와 성능 패널, 스크린샷에 해당하는 명령은 없다.

## 2026-10-31: 업데이트 공백을 없애는 3층과 4층 측정

- Windows `10.0.26300`, 120 DPI, 화면 밖 창, 시험 프로필만 사용했고 동기화는 껐다.

### 3층: 함수 지문으로 주소 다시 찾기

`.pdata`의 모든 함수를 명령어 단위로 해석해 상대 주소를 가리고 해시를 비교했다. 정답은 표의 행과 PDB다. `.48`과
Brave `.62`의 DLL은 이 컴퓨터에 없어서 그 두 쌍은 행에 기록된 앞부분 바이트로만 쟀다.

| 쌍 | 지문 | 유일하고 맞음 | 틀림 | 모호 또는 없음 |
|---|---|---|---|---|
| Chrome `.48` 행 -> `.74` | 앞부분 바이트 | 1/72 (1.4%) | 0 | 71 |
| Brave `.62` 행 -> `.63` | 앞부분 바이트 | 2/63 (3.2%) | 0 | 61 |
| Chrome `150.0.7871.187` -> `156.0.8078.74` | 함수 전체 | 10/67 (14.9%) | 0 | 57 |
| 같은 쌍 | 함수 전체, 오프셋과 즉시값도 가림 | 19/67 (28.4%) | 0 | 48 |
| 같은 쌍 | 처음 32바이트 | 11/67 (16.4%) | 1 | 55 |
| Chrome `.74` -> Brave `.63`(같은 Chromium, 다른 컴파일) | 함수 전체 | 29/59 (49.2%) | 1 | 29 |
| 같은 쌍 | 함수 전체, 양쪽에서 유일 | 29/59 (49.2%) | 0 | 30 |
| Chrome `150.0.7871.187` -> Chrome for Testing `150.0.7871.124` | 함수 전체 | 48/67 (71.6%, 정답 없음) | 알 수 없음 | 19 |

- 패치 차이의 전체 함수 비교는 정답이 있는 쌍이 없다. 정답 없는 마지막 줄이 가장 가까운 값이고 72%에 그친다.
- 표의 주소 약 190개 중 앞부분 바이트가 있는 후크는 88개뿐이다. 나머지는 vtable, 데이터, 함수 안 위치이고, 후크 중
  14개는 해제 정보가 없어 함수 시작점으로 찾을 수 없다. 멤버 오프셋, 슬롯, 열거값은 지문으로 얻지 못한다.
- 모든 함수를 지문으로 만드는 데 한 스레드로 3.2-6.6초 걸린다. 브라우저 시작 시 그만큼 막힌다.
- Rivet 창은 후크가 모두 있어야 한다. 일부만 찾으면 어차피 4층으로 넘어가므로 구현하지 않았다.

### 4층: 검증된 빌드 보관

| 브라우저 | 복사본 실행 | 쿠키 재시작 유지 | 쿠키 접두사 | 확장 | 샌드박스 | Rivet 창과 경고 |
|---|---|---|---|---|---|---|
| Chrome `156.0.8078.74` | 됨 | 됨 | `v10` | 됨(디버그 파이프로 로드) | 시스템판과 같음 | 됨 |
| Edge `156.0.4314.50` | 됨 | 됨 | `v10` | 됨 | 렌더러 잠금 확인 | 됨 |
| Brave `156.1.98.63` | 됨 | 됨 | `v10` | 됨 | 렌더러 잠금 확인 | 됨 |
| Whale `5.39.412.57` | 됨 | 됨 | `w12` | 됨 | 렌더러 잠금 확인 | 됨 |
| Samsung `30.0.4.29` | 됨 | 됨 | `v10` | 됨 | 렌더러 잠금 확인 | 됨 |

- 앱 바운드 암호화는 문제가 되지 않았다. `--user-data-dir`을 준 시험 프로필에서는 시스템 Chrome도 복사본도
  `app_bound_encrypted_key`가 없고 쿠키가 `v10`이다. 그래서 경로 검증을 타지 않는다.
- 같은 프로필을 복사본 `.74`, 시스템 `.74`, 더 오래된 `150.0.7871.187` 순서로 열어도 오류 화면이 없고 쿠키가 전송되었다.
  패치 차이의 낮춤은 실제 빌드가 없어 재지 못했다.
- 프로그램 파일 ACL이 시스템판과 다르다. 설치기는 앱 컨테이너 계정에 읽기를 준다. 복사본 폴더에 같은 두 계정을 준다.
  복사본 폴더는 사용자가 쓸 수 있어 Program Files의 쓰기 보호는 없다.
- 복사본은 496-938 MB이고 Whale처럼 이전 버전 폴더가 남은 브라우저는 현재 버전만 복사한다.
- 복사본에 실행 중인 창의 모듈 중 Program Files에서 온 것은 0개다.
- 실제 브라우저 업데이트 중의 동작은 재현하지 못했다. 표에 없는 실제 빌드(Chrome for Testing `150.0.7871.124`)를 설치된
  브라우저로 지정해 확인했다.

## 2026-11-02: 보관 빌드를 develop에서 끝까지 재현

- Windows `10.0.26300`, 화면 밖 창, 새 시험 프로필, 동기화 끔. `scripts/tests/windows-held-build.ps1`.
- 설치된 브라우저는 시스템 DLL을 `chrome.dll`로 복사한 폴더로 흉내 냈다(버전 `10.0.26100.9711`, 어느 행에도 없음).
  Rivet은 그 폴더를 읽기만 하고, 실제 Chrome `156.0.8078.74`에서 `rivet hold refresh`로 만든 보관 복사본(496 MB)에서
  창을 열었다. 복사본 프로세스의 실행 파일은 보관 폴더 안이고, 시험 프로필의 모든 프로세스가 그렇다.
- Rivet이 붙었고 창 캡처에 사이드바, 위 줄, 아래 오른쪽의 경고("7 days left")가 보였다. 경고 바탕 픽셀 24,724개.
- 첫 발견 시각을 8일 전으로 바꾸면 창이 열리지 않고 이유를 말했다. 보관본이 없으면 창이 열리지 않고 이유를 말했다.
- 보관 복사본에 `Default`, `User Data`, `Cookies`, `Login Data`, `Local State` 같은 이름은 없었다.
- 실제 브라우저 업데이트 중의 동작은 이번에도 재현하지 못했다. 복사는 이미 캐시된 파일에서 1초 걸렸다.

## 2026-11-02: 아무 Chromium 실행 파일에서 Chromium 코드만 찾아 쓸 수 있는가

D8의 새 결정(실행 파일 지정, Chromium 코드만 호출하고 수정, 회사가 바꾼 화면은 따르지 않고 포기)을 코드와 이진에 대 보았다. 1부는 런타임이 지금 쓰는 값의 분류이고, 2부는 그 값을 지정한 실행 파일에서 찾을 수 있는지의 측정이다. 브라우저를 실행하거나 붙이지 않았다. 설치된 파일과 캐시된 공개 PDB만 읽었다.

### 확인한 환경

- Windows `10.0.26300`. 도구는 `.scratch`에만 두었다(PDB 나열은 DbgHelp, 이진 색인은 Python과 numpy, 명령어 읽기는 VS의 `llvm-objdump`).
- 이진: Chrome `156.0.8078.74`(`chrome.dll` 292 MB), Edge `156.0.4314.58`과 `.50`(`msedge.dll` 339 MB), Brave `156.1.98.63`, Whale `5.39.412.57`(`whale.dll` 329 MB, 안의 Chromium `150.0.7871.252`), Samsung Internet `30.0.4.29`(`samsunginternet.dll` 260 MB, Chromium `143.0.7499.194`). 비교용으로 Chrome `150.0.7871.187`(PDB 있음)과 Chrome `143.0.7499.194`(PDB 없음)의 파일.
- PDB: Chrome `.74`와 `150.0.7871.187`, Brave `.63`, Edge `.50`. Edge `.58`의 PDB는 캐시에 없어서 받지 않았다. Whale과 Samsung은 공개 PDB가 없다.
- 행: 런타임에 컴파일된 행(`rivet-rows row`)과 `rows/table.json`의 Edge `.58`. 행의 주소가 Edge `.58`에서 `.50`으로 옮겨진 이름을 잇는 근거는 이전 기록(`chrome-builds port`)이다.

### 1부: 감사

행의 주소 하나마다 그 빌드의 PDB에서 심볼 이름을 읽고, 이름이 Chrome `156.0.8078.74`의 PDB에도 있으면 Chromium 코드로,
아니면 Chromium 밖의 코드로, 이름이 없으면 모름으로 나눴다. PDB로 가른 것은 이름이지 본문이 아니다.

- Chrome 행은 모두 Chromium 코드다.
- 다른 빌드의 행 일부는 Chromium 밖의 코드나 이름 없는 값을 가리켰다. 그런 값을 빼면 일부 브라우저에서 위 줄, 페이지
  영역, 분할, Sidebar를 잃는다. "지원 브라우저는 모든 기능이 같다"는 D8 기준과 맞지 않으므로, 행은 Chromium 함수만
  든다.
- 소스에 상수로 박힌 Chromium 배치도 있다. `views::View`의 부모 `0x198`, 경계 `0x1C8`은 Chrome `156`에서 맞춘 값이고
  다른 빌드에서는 다를 수 있다(예: 부모 `0x190`, 자식 `0x198`, 경계 `0x1C0`). `std::map` 노드(키 `+0x20`, 값 `+0x28`,
  플래그 `+0x38`)는 libc++ 배치이다. 색 공급자 트리 `+0x18`, 노드 키 `+0x20`, 색 `+0x24`. `GURL` 크기 `0x78`.

### 2부: 임의의 실행 파일에서 찾을 수 있는가

#### 먼저 줄이기: Chromium 코드만으로 기능을 하려면 값이 몇 개 필요한가

Chrome `156.0.8078.74` 행에서 (a)로 분류되는 값은 다음과 같다. 함수는 이름으로 중복을 뺐다(같은 함수를 여러 필드가 가리킨다). 구조 정수는 필드 이름으로 고른 값이라 대략이다.

| 능력 | 함수 | vtable | 아이콘 등 데이터 | 구조 정수(오프셋, 슬롯) |
|---|---|---|---|---|
| 프레임, 캡션, 레이어 | 14 | 4 | 1 | 9 |
| 탭 제어 | 13 | 0 | 5 | 1 |
| 주소 열기 | 7 | 0 | 2 | 0 |
| 분할 | 9 | 1 | 1 | 11 |
| Sidebar와 페이지 영역 훅(`side_panel` 전체) | 7 | 4 | 3 | 10 |
| 가로 탭바 숨기기(`vertical`) | 8 | 1 | 1 | 3 |
| 위 줄과 주소(`toolbar`, `location_bar`) | 47 | 15 | 19 | 19 |
| 합계(섹션 간 중복 포함) | 105 | 25 | 32 | 53 |

- "데이터"는 아이콘 데이터와, 해제 정보가 없어 `.pdata`에 시작점이 없는 잎 함수(작은 접근자)를 함께 센 것이다. 뒤의 찾기 측정은 `.pdata`에 있는 함수만 다룬다. 잎 함수는 이 방법으로 시도조차 하지 못했다.
- 이 밖에 함수마다 앞부분 바이트 검사 값이 72개 있다.
- 페이지 영역 훅만 떼어 보면 함수 1개(`BrowserViewTabbedLayoutImpl::CalculateProposedLayout`), vtable 1개, 슬롯 1개, 배치 객체의 멤버 오프셋 약 9개다.
- 가장 작은 능력 묶음(A)은 페이지 영역 훅, 탭 제어, 주소 열기다. 필드 16개(함수 15, vtable 1), 슬롯 1개, 구조 정수 약 10개다. 여기에 가로 탭바 숨기기와 프레임을 더하면(A2) 필드가 약 40개로 는다. 위 줄(FR-12)은 함수 47개, vtable 15개, 데이터 19개로 가장 크고 회사 코드와 가장 많이 얽힌다.
- 줄일 수 있는 후보는 있으나 측정하지 않았다. 탭 열기, 닫기, 전환, 뒤로, 앞으로는 브라우저 창에 `WM_COMMAND`로 `IDC_*` 명령을 보내면 되고, 그 id는 가속기 표에서 읽는다(개발자 모드 단추가 이미 쓰는 길이다). 탭 제목과 주소는 접근성(UI Automation) 트리에서도 읽힌다. 둘 다 주소가 필요 없다. 페이지 영역은 창의 비클라이언트 영역 계산으로 줄이는 길이 있을 수 있다. 이 셋은 가설이고 이 조사에서 실행하지 않았다.

#### 방법

참조는 PDB가 있는 Chrome 빌드(`156.0.8078.74`, `150.0.7871.187`)이고, 참조의 함수 이름과 주소는 PDB가 준다. 대상은 지정한 실행 파일이다. 이름 없는 이진에서 다음 단서로 같은 함수를 찾았다. 명령어를 해석하지 않고 바이트 패턴과 PE 구조만 썼다.

1. 함수 목록: `.pdata`. 핫/콜드로 쪼갠 조각(연쇄 항목)은 뺐다. Edge는 항목 1,011,802개 중 178,284개가 조각이었다.
2. 문자열 참조: `lea reg, [rip+disp32]`가 `.rdata`의 문자열 시작을 가리키는 것. 추적 이벤트 이름(`BrowserFrameViewWin::OnPaint` 등), 지표 이름, 스위치, 경로 문자열이 여기에 든다.
3. 호출 그래프: 함수 시작을 가리키는 `E8`과 `E9`.
4. vtable: `.rdata`와 `.data`에서 함수 포인터가 3개 이상 이어진 구간. 같은 함수가 들어 있는 vtable의 이웃 슬롯으로 대응을 퍼뜨린다.
5. 규칙: 문자열이 참조 쪽에서 3개 이하 함수에만 있을 때만 단서로 쓴다. 대응이 서로 가장 좋은 짝(양방향)이어야 하고, 크기 비가 0.3에서 3.3 사이여야 하며, 두 함수가 한 함수로 몰리면 버린다. 호출 그래프 전파는 지원하는 이웃이 둘 이상이고 절반 이상이 맞아야 한다. 대상 함수 주변 2단계 이웃에만 전파했다.
6. 정답: PDB가 있는 대상(Chrome `.74`, `150`, Edge `.50`, Brave `.63`)은 같은 이름의 주소다. PDB가 없는 대상(Whale, Samsung, Edge `.58`)은 컴파일된 행의 같은 필드다. 이 행들은 사람이 검증했거나 생성기가 옮긴 값이다. 한 이름이 여러 주소를 가져 정답이 하나로 정해지지 않는 값은 채점에서 뺐다. 행의 `signatures[...]` 배열은 필드 순서가 빌드마다 달라 채점에서 뺐다(이름 있는 필드가 같은 값을 가진다).

#### 표 3: 함수 찾기 결과

대상 필드는 Chrome `156` 행의 (a) 함수 필드이고, 함수는 이름으로 중복을 뺐다. Whale과 Samsung은 행에 기록된 값이 적어서 채점한 함수가 적다(위 줄이 없다).

| 참조 -> 대상 | Chromium 거리 | 채점 | 맞음 | 틀림 | 못 찾음 | vtable 시작 맞음 / 틀림 / 못 찾음 |
|---|---|---|---|---|---|---|
| Chrome 156 -> Brave 156.1.98.63 | 같은 마일스톤, 다른 회사 빌드 | 94 | 44 (47%) | 0 | 50 | 1 / 0 / 10 |
| Chrome 156 -> Edge 156.0.4314.50 | 같은 마일스톤, 다른 회사 빌드 | 94 | 32 (34%) | 4 | 58 | 2 / 0 / 12 |
| Chrome 156 -> Edge 156.0.4314.58 | 같음 | 90 | 30 (33%) | 5 | 55 | 2 / 0 / 11 |
| Edge .50 -> Edge .58 | 같은 회사, 패치 빌드 | 103 | 51 (50%) | 2 | 50 | 2 / 0 / 41 |
| Chrome 156 -> Chrome 150 | 6 마일스톤 뒤로 | 85 | 36 (42%) | 1 | 48 | 0 / 0 / 8 |
| Chrome 150 -> Chrome 156 | 6 마일스톤 앞으로 | 82 | 37 (45%) | 4 | 41 | 0 / 0 / 4 |
| Chrome 150 -> Whale 5.39.412.57 | 같은 마일스톤(150), 다른 회사 | 22 | 10 (45%) | 1 | 11 | 0 / 0 / 0 |
| Chrome 156 -> Whale | 6 마일스톤 + 다른 회사 | 27 | 12 (44%) | 1 | 14 | 0 / 0 / 2 |
| Chrome 150 -> Samsung 30.0.4.29 | 7 마일스톤 + 다른 회사 | 13 | 7 (54%) | 1 | 5 | 0 / 0 / 0 |
| Chrome 156 -> Samsung | 13 마일스톤 + 다른 회사 | 18 | 6 (33%) | 1 | 11 | 0 / 0 / 1 |

- 맞춘 방법의 비중(Brave): 문자열 13, 호출 그래프 29, vtable 2. 문자열 단서가 있는 함수는 참조 쪽 Chromium 함수 96개 중 19개뿐이다. 단서 56개의 종류는 추적 이벤트나 메서드 이름 13, 지표 이름 7, 스위치 6, 경로 1, 그 밖에 29이다. 나머지 77개 함수는 문자열이 없어서 호출 그래프와 vtable 이웃에만 기대고, 그것도 이미 찾은 이웃이 둘 이상 있어야 한다.
- 문자열만 쓰면(Brave) 채점한 함수 필드(중복 포함) 126개 중 104개에 단서가 없고, 16개를 맞고, 2개를 틀리고(`GetLayoutConstant`와 `GetLayoutInsets`가 한 함수로 몰림), 3개는 동점으로 모호했다. 씨앗 문자열 후보 67,088개 중 14,684개(22%)가 동점이라 버려졌다.
- 거리와 성공률의 관계는 뚜렷하지 않다. 같은 마일스톤의 Brave가 47%, 13 마일스톤 떨어진 Samsung이 33%에서 54%다. 못 찾는 이유는 거리가 아니라 함수의 성질(문자열이 없고, 찾은 이웃이 적음)이다. 틀린 비율은 대상이 멀거나 회사 코드가 많이 다를수록 올랐다(Brave 0, Edge 4에서 5).

#### 틀린 결과 (가장 나쁜 결과)

틀린 것은 대부분 이웃이나 형제 함수로 번진 것이다. 아래는 크기 비, 쌍방향, 중복 대상 검사를 모두 통과한 값이다. Edge `.58` 쌍의 틀림 하나는 정답이 Chromium 함수가 아니라서 표에서 뺐다.

| 쌍 | 참조 함수 | 잘못 고른 것 | 어떻게 |
|---|---|---|---|
| 156 -> Edge | `BrowserFrameViewWin::OnPaint` | `views::View::OnPaint` | vtable 이웃 943개가 지지했지만 슬롯이 하나 어긋남 |
| 156 -> Edge | `views::Widget::LayoutRootViewIfNecessary` | 같은 함수 안의 추적 람다 | 문자열이 컴파일러가 빼낸 람다에 있음 |
| 156 -> Edge | `TabGroupModel::GetTabGroup` | `TabGroup::ListTabs` | 호출 그래프, 지지 7 |
| 156 -> Edge | `views::Widget::GetWidgetForNativeWindow` | Edge 행은 `GetWidgetForNativeView`를 쓰는데 대상은 `GetWidgetForNativeWindow`를 골랐다. Chrome에서는 둘이 한 주소로 접혀 있고 Edge에서는 따로다 | 호출 그래프, 지지 5 |
| 150 -> 156 | `views::View::GetColorProvider` | `views::Widget::GetColorProvider` | 호출 그래프, 지지 6 |
| 150 -> 156 | `TabStripModel::CloseWebContentsAt` | `TabStripModel::CloseWebContents` | 호출 그래프, 지지 4 |
| 150 -> 156, 156 -> 150 | `views::FlexLayout::CalculateProposedLayout` | `views::FillLayout::CalculateProposedLayout` | vtable 이웃, 지지 4 |
| 150 -> 156 | `views::Widget::ThemeChanged` | `ParentThemeObserver::OnColorProviderChanged` | 호출 그래프, 지지 2 |
| 156 -> Samsung | `GURL::GURL` | 다른 중복정의(Samsung 행은 UTF-16 판) | 호출 그래프, 지지 554 |
| 156 -> Whale | `BrowserFrameViewWin::OnPaint` | 같은 이름의 다른 함수 | 문자열 |
| 150 -> Samsung | `TabStripModel::GetTabGroupForTab` | 이웃 함수 | 호출 그래프, 지지 3 |
| Edge .50 -> .58 | Chromium 밖의 함수 둘 | 무관한 함수 둘 | vtable 이웃 1490개와 943개가 지지했는데 어긋남 |

- 호출 그래프 맞춤의 지지 수로 거르는 방법을 쟀다. 지지 8 이상만 받으면 맞은 133개 중 68개를 남기고 틀린 6개 중 1개가 남는다. vtable 맞춤은 지지 4 이상이면 맞은 13개 중 3개만 남고 틀린 3개가 그대로다. 문턱 하나로는 갈리지 않는다.
- 이 틀린 값이 실제로 쓰이면 브라우저가 죽을 수 있다. 앞부분 바이트를 행에 미리 적어 두는 지금의 검사는 새 빌드에는 쓸 수 없다. 바이트를 모르기 때문이다. 이 조사에서 틀린 값을 모두 거르는 검사는 찾지 못했다. 거르려면 찾은 함수를 호출해 결과를 보거나 본문을 명령어 수준으로 비교하는 길이 남아 있고, 측정하지 않았다.

#### vtable, 슬롯, 오프셋, 열거값

- vtable 시작: 구간 안에서 같은 함수를 가진 이웃으로 시작 위치를 정한다. 맞춘 것은 Brave 1/11, Edge `.50` 2/14, 나머지 쌍 0이다. 정답이 하나가 아닌 값은 뺐다(Brave 14개). 이름은 같고 주소가 여럿인 vtable(기반 클래스 하위 객체의 vtable)이 많다.
- 슬롯: vtable 시작과 함수가 정해지면 슬롯은 계산된다. 행의 vtable 주소와 함수 주소로 슬롯을 계산하니(Chrome 10쌍, Brave 10쌍, Edge 8쌍) 28쌍이 모두 행의 슬롯과 같았다. 그래서 슬롯은 vtable과 함수를 찾는 만큼만 찾아지고, 이 조사에서는 vtable이 걸림돌이다. 슬롯 값은 회사마다 다르다(`on_paint_slot`이 Chrome 0x59, Edge 0x65).
- 멤버 오프셋: 지문으로 못 얻는다. 대신 읽는 함수의 명령어를 참조와 대상에서 정렬해 같은 자리의 변위를 읽어 보았다(`llvm-objdump`, 명령어 형태 열의 차이 정렬). 필드와 읽는 함수 쌍은 행 주석을 읽고 20쌍을 골랐다.

| 대상 | 맞음 | 틀림 | 정렬 못 함 | 참조에서 읽는 함수가 그 오프셋을 안 만짐 | 값이 Chrome과 달랐는데 맞춘 수 |
|---|---|---|---|---|---|
| Brave `.63` | 6 | 2 | 4 | 7 | 0 |
| Edge `.50` | 5 | 0 | 8 | 7 | 1 (`tabs.browser_in_view` 0x560에서 0x618) |
| Whale (150 참조) | 2 | 0 | 0 | 0 (18쌍은 행에 값이 없음) | 1 (`browser_window_in_view` 0x1f0에서 0x210) |

  Brave의 틀린 2개는 `mini_toolbar_member`(0x440)와 `outline_member`(0x448)가 서로 바뀐 것이다. 컴파일러가 이웃 멤버의 읽기 순서를 바꿔서 정렬이 어긋났다. 값은 맞고 이름이 바뀌었으므로 크래시 후보다.
- Chrome 156의 값을 그대로 쓰면(구조 정수 약 55개): Brave는 50개가 같고 5개가 다르다. Edge는 54개 중 15개만 같다. Whale은 18개 중 5개, Samsung은 5개 중 1개다. Edge는 클래스에 멤버를 더해서 대부분의 오프셋이 밀렸다(예: `BrowserView`의 `Browser` 멤버 0x560에서 0x618, `MultiContentsView`의 활성 멤버 0x3c8에서 0x420).
- 열거값(색 ID, `LayoutConstant`, 창 기능 값, 항목 id)과 아이콘 데이터 주소 32개는 시도하지 않았다. 이전 기록에서 Edge는 이진의 `{id, 이름}` 표로 색 ID를 증명했다.

#### 시간과 캐시

- 한 이진의 색인(Python과 numpy, 한 스레드, 파일은 OS 캐시에 있음): 9-16초. 문자열 씨앗 맞춤 11-18초. 호출 그래프와 vtable 전파 29-132초. 합계 한 대상당 약 50-170초. Rust로 옮기면 얼마나 줄는지는 측정하지 않았다. 이전에 잰 함수 지문의 3.2-6.6초보다 한 자릿수 크다.
- 결과는 파일 내용만의 함수라서 파일 해시로 캐시할 수 있다. 파일 전체 SHA-256은 0.4-0.5초(캐시된 파일 기준, 처음 읽을 때는 디스크 속도에 달림)다. 캐시에는 필드별 주소와 검사 바이트만 넣으면 수 KB다. 같은 빌드를 다시 열면 해시 계산과 바이트 검사만 든다.
- 첫 실행이 문제다. 모르는 빌드를 처음 여는 데 최소 1분이 더 든다. NFR-6은 일반 브라우저로 열리는 시간 0을 요구하므로, 이 시간 동안은 보관 빌드로 열거나 창을 열지 않아야 한다. 그리고 찾기가 끝나도 위 표의 성공률로는 적용할 수 없다.

#### 마일스톤과 빌드 사이가 얼마나 벌어져도 되는가

- 이 빌드들이 보여 주는 것은 일정한 한계점이 아니라 사라지지 않는 천장이다. 같은 마일스톤(Brave)에서 47%, 6마일스톤 차이(Chrome 150과 156)에서 42-45%, 13 마일스톤 차이(Samsung)에서 33-54%다. 같은 회사의 패치 빌드 사이(Edge `.50`에서 `.58`)도 50%다. 이 방법에서 못 찾는 함수는 거리와 상관없이 못 찾는다.
- 틀리는 비율은 거리와 회사 차이에 따라 움직였다. Brave 0/44, 같은 회사의 Chrome 사이 1-4/36-37, Edge 4-5/30-32, Samsung과 Whale 1/6-12.
- 브라우저가 구조를 바꾼 경우는 거리와 무관하게 깨진다. 함수 항목이 핫/콜드 조각으로 많이 쪼개지거나, 멤버 오프셋이 밀리거나, 열거값이 밀리면 찾기가 어긋난다.

#### 측정하지 않은 것

- 어떤 브라우저도 실행하거나 붙이지 않았다. 찾은 주소가 실제 호출에서 동작하는지, 본체로 훅을 옮겨도 Brave와 Edge에서 기능이 되는지 모른다.
- Rust로 옮긴 구현의 시간과 메모리. 위 시간은 Python 시제품이다.
- 명령어 수준 지문(본문 해시)은 다시 재지 않았다. 앞선 기록의 값(28-49%)을 쓴다. 같은 회사의 인접 빌드에서는 본문 해시가 더 잘 맞을 것으로 보이나 이 조사의 방법이 아니다.
- 이 열 가지 쌍은 5개 회사의 빌드 하나씩이다. 다른 Chromium 계열(Opera, Vivaldi, 직접 빌드한 Chromium)은 설치되어 있지 않아 재지 못했다.
- Whale과 Samsung의 정답은 사람이 만든 행이라서 PDB로 검증한 정답이 아니다. Edge `.58`은 `.50` PDB의 이름과 행으로만 가렸다. Whale과 Samsung은 행에 기록된 값이 적어서(위 줄 없음) 채점한 함수가 27개와 18개뿐이다.
- 접근성, `WM_COMMAND`, 비클라이언트 영역으로 주소 없이 기능을 만드는 길(가설)은 실행하지 않았다.
- 오프셋 20쌍은 내가 행 주석을 읽고 고른 쌍이다. 쌍을 잘못 고른 7개는 방법의 실패가 아니라 쌍의 실패다.
- 열거값, 색 ID, 아이콘 데이터 32개는 시도하지 않았다.

### 결론

달성 가능하지 않다. 판정하는 수는 가장 쉬운 경우의 값이다. 같은 마일스톤의 Brave에서 함수 94개 중 44개(47%)만 맞았고, 가장 작은 능력 묶음(페이지 영역 훅, 탭 제어, 주소 열기의 함수 필드 16개)에서도 4개(`GetContentWindowForHWND`, `OpenURLParams`, 레이아웃 vtable 등)가 빠졌다. 어느 대상(Brave, Edge, Whale, Samsung)에서도 이 묶음이 완전하지 않았다. 지원하려면 모든 값이 맞아야 하는데 절반을 못 찾는다. 더 나쁜 것은 틀린 값이다. Edge와 Chrome 150에서 156으로 가는 쌍에서는 맞은 값 30-37개에 틀린 값 4-5개가 섞였고, 어떤 문턱으로도 갈리지 않았다. 멤버 오프셋은 지문이 아니며, 읽는 코드를 정렬하는 방법도 20쌍 중 Brave에서 2개를 서로 바꿔 틀렸다.

감사 쪽 수치도 같은 결론을 가리킨다. Chromium 밖의 코드를 빼면 일부 브라우저는 위 줄, 페이지 영역, 분할, Sidebar를 잃는다. D8의 "지원 브라우저는 모든 기능이 같다" 기준을 지키려면 Chromium 코드만으로 되는 경로가 필요하다.

#### 가장 작은 다음 걸음

주소 없이 되는 Chromium 경로를 먼저 시험한다. 새 시험 프로필(동기화와 로그인 끔)로 화면 밖 창을 하나씩 열어서 Chrome과 Brave에서 다음 둘만 확인한다.

1. `WM_COMMAND`의 `IDC_*`(탭 열기, 닫기, 전환, 뒤로, 앞으로, 새로 고침)와 접근성 트리의 탭 목록과 주소가 읽히고 동작하는가. 되면 탭 제어와 주소 열기의 함수 20개가 필요 없어진다.
2. 서브클래스한 창 프로시저의 `WM_NCCALCSIZE`로 왼쪽 열을 줄이면 Chromium이 줄어든 클라이언트 영역에 스스로 배치하는가. 되면 페이지 영역 훅(함수 1개, vtable 1개, 오프셋 약 9개)이 필요 없어진다.

둘 다 되면 최소 묶음이 주소 0개가 되고, 그때 이 문서의 찾기 측정은 위 줄과 분할만 남는다. 안 되면 임의 실행 파일 지원은 접는 쪽으로 간다.

## 2026-11-04: app window와 두 번째 페이지 뷰 시험

app window 시제품. 코드는
`rivet/runtime/windows/src/features/app_window.rs`와 `scripts/tests/appspike/`다. 대상은 Chrome `.74`, Edge `.58`,
Brave `.63`, Whale `.57`, Samsung `.29`이고 화면 밖 시험 프로필(동기화와 로그인 끔)로 열었다.

### 측정

- 실행: visible 실행이 `--app=<시작 페이지>`로 열린다.
- 탭: Chromium `TabStripModel::DetachTabAtForInsertion`과 `InsertDetachedTabAt`로 탭을 app window에 옮긴다. Chrome,
  Brave, Edge는 공개 이름으로, Whale은 Chrome 150의 같은 함수와 모양을 맞춰 찾았다. Samsung(Chromium 143)은 넣기 함수를
  찾지 못했다. 브라우저 종류 값을 바꾸는 방법은 Chrome에서만 됐다.
- `AddAndReturnTabAt`와 `IDC_NEW_TAB`은 별도 일반 창을 연다. Edge에서는 탭 창이 없으면 브라우저가 종료된다.
- 두 번째 뷰: `ContentsWebView` 생성자, 크기, `GetBrowserContext`, `AddChildViewAtImpl`, `SetBoundsRect`,
  `SetWebContents`(vtable 칸 0x67)를 브라우저마다 찾았다. Chrome은 `operator new(0x528)`, Whale은 크기 538, Samsung은
  4C0이다. Samsung은 `SetBoundsRect`가 없어 view+0x1C0에 범위를 직접 쓴다.
- 부모는 BrowserView 자신의 View여야 한다. BrowserWindow 부분을 부모로 쓰면 브라우저가 종료되었다.
- 다섯 브라우저 모두에서 두 번째 뷰의 페이지가 주 페이지 옆에 그려졌다.
- 같은 창의 탭 페이지는 클릭하면 탭 활성화 때문에 주 페이지 뷰로 돌아간다. 다른 창의 페이지는 원래 페이지 뷰에서
  `SetWebContents(null)`로 먼저 놓으면 클릭해도 남는다.
- 클릭은 모든 브라우저에서 페이지에 들어갔다. 키 입력은 Edge에서만 들어갔다.
- Chrome은 이 모드의 시작 배치에서 약 세 번에 한 번 runtime +0x1BA5F에서 종료된다. 원인은 모른다.
- Edge는 페이지 위에 약 55px 회색 띠, Samsung은 약 55px 크림색 띠를 보인다.
- 비밀번호 저장 말풍선은 Chrome, Brave, Whale, Samsung에서 뜨지만(Edge는 안 뜸) 창이 아니라 모니터 가장자리에 붙는다.

### 한계

- 주소는 빌드마다 사람이 찾았고 서명된 표(`rows/table.json`, `builds/*.rs`)의 행이다. 임의 실행 파일 지원의 근거가 아니다.
- 상단 줄, 제안, 보안 표시, 권한 말풍선, IME는 시험하지 않았다.

### 결론

app window와 두 번째 `ContentsWebView`는 다섯 브라우저 모두에서 그려진다. 회사 코드 없이 분할과 Sidebar를 만들 수 있는
길이 있다. 탭 소유, 키 입력, 말풍선 위치, Chrome 시작 종료, 회사 띠는 남은 문제다.

## 2026-11-09: 빌드 값 없이 HWND에서 view에 닿는 길 (Chromium M150 공개 소스)

Whale 5.39.412.57은 Chromium 150.0.7871.252다. 상단 줄 아래 페이지 위치를 빌드마다 사람이 찾은 주소 없이 읽을 수
있는지 공개 소스 `refs/branch-heads/7871`에서 확인했다. 받은 파일은 `.scratch/m150-src/`에 있다.

### 확인한 사실

- `gfx::SetWindowUserData`는 `GWLP_USERDATA`에 `WindowImpl*`를 넣고, `WindowImpl`의 창 프로시저는
  `CHECK_EQ(this, GetWindowUserData(hwnd))`로 같은 값을 다시 읽는다
  ([window_impl.cc](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/gfx/win/window_impl.cc),
  [hwnd_util.cc](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/gfx/win/hwnd_util.cc)).
- `views::HWNDMessageHandler`는 `gfx::WindowImpl`을 상속한다
  ([hwnd_message_handler.h](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/views/win/hwnd_message_handler.h)).
  그래서 브라우저 HWND의 `GWLP_USERDATA`는 빌드 값 없이 `HWNDMessageHandler`를 준다.
- `ui::ViewProp`은 Win32 창 속성이 아니라 프로세스 안의 `std::set`이다
  ([view_prop.cc](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/base/view_prop.cc)).
  Chromium 함수 없이는 읽을 수 없다.
- 그 다음 고리(`HWNDMessageHandler::delegate_`, `DesktopWindowTreeHostWin`, native widget, `Widget`, `root_view_`)는
  모두 멤버 위치이고 빌드마다 다르다.
- `ui::metadata::MetaDataProvider`의 가상 함수는 소멸자와 `GetClassMetaData()` 하나다. `View`는 이것을 여덟 번째
  부모로 상속한다. `ClassMetaData`는 `type_name_`을 실행 파일 안 정적 문자열의 `string_view`로 갖고,
  `FindMemberData`로 X, Y, Width, Height를 준다
  ([metadata_types.h](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/base/metadata/metadata_types.h),
  [view.h](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/ui/views/view.h)).
- M150 `browser_view.cc`는 BrowserView에 `kBrowserViewElementId`를 붙인다. 이 가지에는 `kTopContainerElementId`와
  contents container id가 없다
  ([browser_view.cc](https://chromium.googlesource.com/chromium/src/+/refs/branch-heads/7871/chrome/browser/ui/views/frame/browser_view.cc)).
  M150의 페이지 view는 id가 아니라 메타데이터 클래스 이름으로 찾아야 한다.

### 결론

빌드 값 없이 들어가는 입구는 `GWLP_USERDATA`의 `HWNDMessageHandler`다. 그 뒤는 메모리만 읽는 탐색으로 포인터
고리를 따라가고, 끝이 메타데이터 이름 `RootView`인 view일 때만 받아들이는 방법이 남는다. 후보에 가상 함수를 부르기
전에 vtable이 모듈의 읽기 전용 데이터에 있고 칸이 코드 구역을 가리키는지 확인해야 한다. 찾은 위치는 Chrome, Edge,
Brave, Samsung의 기존 행과 같아야 쓸 수 있다.

## 2026-11-09: 메타데이터로 view 찾기, 읽기 전용 시험 결과

`rivet` 런타임에 디버그 명령 `reflect`를 두고 다섯 브라우저에서 읽기만 하는 걸음을 시험했다. 브라우저 함수는
부르지 않았고 작업 스레드에서 500 ms 안에 끝냈다. 기록은 `.scratch/reflect1`부터 `.scratch/reflect9`, `.scratch/post58`에
있다.

### 된 것

- 클래스 이름 읽기: `MetaDataProvider` 부분의 vtable 두 번째 칸(`GetClassMetaData`)을 부르지 않고 해석한다. 칸은
  `sub rcx, n` 뒤 `jmp`인 조정 thunk이고, 끝의 `MetaData()`는 정적 포인터를 `mov rax, [rip+d]`로 읽는다
  (M150 `metadata_macros_internal.h`의 `BEGIN_METADATA_INTERNAL`). 그 포인터가 가리키는 힙의 `ClassMetaData`에서
  `type_name_` 문자열을 읽는다. 다섯 브라우저 모두 `BrowserView`, `View`, `FocusRing` 같은 이름을 읽었다.
- `GWLP_USERDATA`의 `HWNDMessageHandler`에서 포인터 필드를 따라가 `BrowserView`에 닿는다. 다섯 브라우저 모두
  170에서 520개 객체 안에서 닿았다(한 번은 Brave가 시간 안에 못 닿았다).
- `BrowserView`의 `View` 부분 안에서 `MetaDataProvider` 위치: Chrome과 Brave 840(0x348), Samsung 824(0x338),
  Whale 248(0xF8).

### 확인한 행 값과의 비교

- view.h(M150)의 멤버 순서는 `widget_`, `parent_`, `children_`(view의 `std::vector`), 그다음 `bounds_` 앞의 몇 필드다.
  자식마다 `parent_`가 그 view를 가리킨다는 점으로 자기 자식 목록을 찾는 방법을 시험했다.
- Samsung 30.0.4.29와 Whale 5.39.412.57: 이 방법이 행이 아는 browser view와 같은 주소를 찾았다. 자식 목록은
  +0x198, `parent_`는 +0x190이다. 공통 상수 `View::PARENT`(0x198)는 이 두 빌드에서는 `children_`의 시작이다.
- Chrome 157, Brave 157, Edge 157: 같은 방법으로 자식 목록을 찾지 못했다. 행의 +0x198이 가리키는 객체는 어느
  부분도 view 클래스 이름으로 읽히지 않고, 그 객체의 필드 +0x338과 +0x358이 browser view를 가리킨다.
  +0x198은 이 빌드들에서 `parent_`가 아닐 수 있다.

### 결론

메타데이터로 이름을 읽고 `BrowserView`에 닿는 데까지는 빌드 값 없이 된다. Chrome, Brave, Edge에서 행과 같은 값을
내는 부모와 자식 목록 규칙은 아직 없다. Whale 상단 줄, Sidebar 열기 단추, 전체화면 Sidebar, 두 번째 페이지 키 입력은
페이지 영역 배치 hook이 없어 생기는 알려진 공백으로 남긴다.
