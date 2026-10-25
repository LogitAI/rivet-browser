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
