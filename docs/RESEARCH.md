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
