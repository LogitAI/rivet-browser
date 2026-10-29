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

## 2026-10-29: Naver Whale 5.39.412.57 지원 가능성

### 확인한 환경

- Windows `10.0.26300`, Naver Whale `5.39.412.57`, 비교 기준 Google Chrome `156.0.8078.48`.

### 결과

- 브라우저 코드는 `5.39.412.57\whale.dll`(344 MB)에 있고 `whale.exe`는 런처다. 바이너리에 `Chrome/150.0.7871.252`
  문자열이 있어 Chrome 156과 6 마일스톤 차이가 난다.
- 공개 심볼은 없다. PDB는 `whale.dll.pdb`(GUID `0054ADE0-34DF-4E85-4C4C-44205044422E`, age 1)이고 Microsoft와
  Google 심볼 서버는 404를 돌려준다.
- Chromium은 RTTI 없이 빌드되어 뷰 클래스의 vtable을 RTTI 이름으로 찾을 수 없다.
- Chrome 156의 함수를 정규화한 명령어 열, 유일한 프롤로그, 추적 이벤트 문자열, 호출 그래프, 코드 군집, vtable
  항목 지문으로 맞췄다. 런타임이 쓰는 함수 108개 중 높음 19, 중간 23, 낮음 51, 못 찾음 15였다. 정답과 대조하지
  못해 확신도는 검증된 값이 아니다.
- 함수 외에도 구조체 오프셋, vtable 슬롯 번호, 아이콘 경로 데이터, 함수 안 패치 위치가 다르다. 예를 들어
  `GetTopInset`의 오프셋 `0x340`, `0x1f0`, `0x330`은 `0x338`, `0x210`, `0x328`이 되었고, `CalculateProposedLayout`
  슬롯은 16에서 15로 옮겼다.
- 자동 맞춤만으로는 Whale을 지원할 수 없다. Chrome 150.0.7871.x 공식 빌드와 PDB를 기준으로 쓰면 차이를 패치
  수준으로 줄일 수 있다.

## 2026-10-29: Brave 156.1.98.62 지원 가능성

### 확인한 환경

- Windows `10.0.26300`, Brave `156.1.98.62`(Chromium `156.0.8078.74`), 비교 기준 Google Chrome `156.0.8078.48`.

### 결과

- 브라우저 코드는 `156.1.98.62\chrome.dll`에 있고 파일 버전 리소스는 `156.1.98.62`이다.
- Brave는 심볼을 공개한다. `https://brave-symbols.s3.brave.com/chrome.dll.pdb/<GUID+age>/chrome.dll.pdb`에 gzip으로
  올라 있다(GUID `2FD98BDE5CE2F69C4C4C44205044422E`, age 1). Microsoft와 Google 서버는 404를 돌려준다. 그래서 Edge처럼
  심볼로 행을 채웠다.
- 창은 `BraveBrowserFrameViewWin`, `BraveBrowserView` 서브클래스이다. 프레임 vtable의 `OnPaint`와 `GetTopInset`은
  Brave의 재정의를 가리킨다. 슬롯 번호와 주요 오프셋은 Chrome 156과 같다.
- Chromium 색 ID는 Brave 자체 ID 때문에 17(`0x11`) 밀렸다. 캡션 글리프 ID는 `0x67D`, `0x67E`, `0x67F`이다.
- 프레임 그리기 훅은 `ColorProvider::GetColor`를 부른다. 이 주소가 비어 있으면 브라우저가 죽는다.
- 프레임, 캡션, 패널 클립, 클라이언트 영역 예약, 탭 목록, 시작 페이지 열기가 동작한다. 가로 탭 줄을 숨기면 캡션
  버튼도 사라져서 켜지 않았다. 툴바, 세로 탭, 사이드 패널, 분할 보기는 아직 꺼져 있다.
