---
name: tauri-fast-verify
description: Tauri 프로젝트의 변경 내용을 가장 빠르고 적절한 수준으로 확인한다. 화면 확인, Rust 컴파일 확인, 로컬 앱 수동 테스트, 최종 배포 빌드 요청에 사용한다.
---

# Tauri 빠른 검증

## 목적

검증 목적에 맞는 가장 가벼운 명령부터 사용한다. 매번 전체 release 빌드와 DMG 생성을 반복하지 않는다.

## 시작 전 확인

1. `git status --short`로 기존 변경을 확인한다.
2. `node --version`, `pnpm --version`, `cargo --version`으로 필요한 도구를 확인한다.
3. 변경 범위가 프런트엔드인지 Rust 백엔드인지 구분한다.

## 검증 단계 선택

### 프런트엔드만 변경한 경우

```bash
pnpm typecheck
pnpm exec vitest run <관련 테스트 파일>
pnpm dev
```

- `pnpm dev`는 브라우저에서 화면만 빠르게 확인할 때 사용한다.
- 전체 테스트는 영향 범위가 넓거나 최종 확인이 필요할 때만 실행한다.

### Tauri IPC 또는 실제 앱 동작을 확인하는 경우

```bash
pnpm tauri dev
```

- 기본 수동 테스트 명령이다.
- 프런트엔드는 저장하면 바로 반영되고, Rust는 변경된 부분만 다시 컴파일한다.
- 설치 파일이나 `.app` 묶음이 필요하지 않다면 이 단계에서 끝낸다.

### Rust 코드가 컴파일되는지만 확인하는 경우

```bash
cd src-tauri && cargo check
```

- 실행 파일을 만들지 않으므로 release 빌드보다 빠르다.
- Rust 로직을 바꿨다면 가능하면 관련 `cargo test`도 실행한다.

### 사용자가 직접 실행할 `.app`이 필요한 경우

```bash
pnpm tauri build --debug --bundles app
```

- debug 빌드는 release 최적화를 생략해 더 빠르다.
- macOS 산출물은 `src-tauri/target/debug/bundle/macos/`에서 확인한다.
- 로컬 실행에 서명이 필요하면 생성된 앱에만 ad-hoc 서명을 적용한다.

### 최종 배포 파일이 필요한 경우

```bash
pnpm tauri build --bundles app
```

- release 빌드는 최종 배포 확인이나 사용자의 명시적 요청이 있을 때만 실행한다.
- DMG가 명시적으로 필요할 때만 `--bundles dmg` 또는 전체 번들을 사용한다.

## 시간 낭비 방지 규칙

- `pnpm tauri build`는 내부에서 `pnpm build`를 실행하므로 바로 전에 같은 프런트엔드 빌드를 중복 실행하지 않는다.
- 수동 테스트 요청에 DMG를 만들지 않는다. `.app` 또는 `pnpm tauri dev`면 충분하다.
- 빌드가 실패하면 성공한 단계와 실패한 포장 단계를 구분한다. 예를 들어 `.app` 생성 후 DMG만 실패했다면 앱 빌드까지 실패했다고 보고하지 않는다.
- 기존 빌드 산출물과 사용자 변경을 삭제하거나 되돌리지 않는다.

## 결과 보고

다음 항목만 간단히 알린다.

- 사용한 검증 단계와 선택 이유
- 성공·실패한 명령
- 수동 테스트용 산출물의 절대 경로
- 실행하지 못한 검증과 그 이유
