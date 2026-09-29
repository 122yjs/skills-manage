# k-skill 자동 출처 인식 검증

2026-09-28: 실제 Hermes 설치 파일 60개를 읽고 SQLite 백업 API로 만든 복제 DB에만 기록했다. 실제 설치 경로를 유지하여 앱의 판별 함수를 호출했다.

## 결과

- 자동 연결: 30/60개. 모두 nomadamas/k-skill, 설치 버전 unknown.
- 나머지 30개는 미확인. 같은 폴더에 있다는 이유로 연결하지 않았다.
- 각 설치의 전체 파일 목록, SHA-256, 실행 권한이 판별 전후 모두 같았다.
- 저장소 조회 캐시 10개, 실패 1개 포함. HTTP 요청 수와는 다르다.
- 소요 시간: 49.4초.
- 설치 안내문의 출처 + 이름과 설명 + 고유 내용 일치를 요구한다. 본문 2문단, 400자, 양쪽 50% 이상 또는 256바이트 이상 보조 파일 전체 일치. 공통 문구와 라이선스는 제외한다.
- korea-weather, seoul-subway-arrival 등은 현재 본문과 일치가 부족하다. 과거 커밋 탐색은 아직 없다.
- 60개 모두 k-skill 출처라는 뜻은 아니다.

## 대상별 결과

| 설치 ID | 결과 | 문단 | 문자 |
|---|---|---:|---:|
| blue-ribbon-nearby | 미확인 | — | — |
| bunjang-search | 미확인 | — | — |
| catchtable-sniper | 미확인 | — | — |
| cheap-gas-nearby | 출처 연결, 버전 미확인 | 15 | 2199 |
| corporate-registration-consulting | 미확인 | — | — |
| coupang-product-search | 미확인 | — | — |
| court-auction-notice-search | 미확인 | — | — |
| daiso-product-search | 미확인 | — | — |
| delivery-tracking | 출처 연결, 버전 미확인 | 50 | 8138 |
| fine-dust-location | 출처 연결, 버전 미확인 | 17 | 1455 |
| foresttrip-vacancy | 미확인 | — | — |
| geeknews-search | 출처 연결, 버전 미확인 | 40 | 7998 |
| han-river-water-level | 출처 연결, 버전 미확인 | 13 | 1279 |
| hipass-receipt | 미확인 | — | — |
| household-waste-info | 출처 연결, 버전 미확인 | 20 | 2693 |
| iros-registry-automation | 미확인 | — | — |
| joseon-sillok-search | 출처 연결, 버전 미확인 | 57 | 14951 |
| k-dart | 출처 연결, 버전 미확인 | 54 | 9869 |
| k-schoollunch-menu | 출처 연결, 버전 미확인 | 18 | 2435 |
| k-skill-setup | 미확인 | — | — |
| kakao-bar-nearby | 미확인 | — | — |
| kakaotalk-mac | 미확인 | — | — |
| kbl-results | 출처 연결, 버전 미확인 | 11 | 1043 |
| kbo-results | 출처 연결, 버전 미확인 | 13 | 988 |
| kleague-results | 출처 연결, 버전 미확인 | 11 | 1242 |
| korea-weather | 미확인 | — | — |
| korean-character-count | 출처 연결, 버전 미확인 | 46 | 5710 |
| korean-consumer-disputes | 미확인 | — | — |
| korean-jangbu-for | 미확인 | — | — |
| korean-law-search | 미확인 | — | — |
| korean-patent-search | 출처 연결, 버전 미확인 | 37 | 12156 |
| korean-privacy-terms | 출처 연결, 버전 미확인 | 16 | 3894 |
| korean-scholarship-search | 미확인 | — | — |
| korean-slang-writing | 출처 연결, 버전 미확인 | 29 | 7376 |
| korean-spell-check | 출처 연결, 버전 미확인 | 90 | 13173 |
| korean-stock-search | 출처 연결, 버전 미확인 | 20 | 3918 |
| ktx-booking | 미확인 | — | — |
| lck-analytics | 출처 연결, 버전 미확인 | 26 | 2868 |
| lh-notice-search | 미확인 | — | — |
| library-book-search | 미확인 | — | — |
| lotto-results | 출처 연결, 버전 미확인 | 10 | 1022 |
| market-kurly-search | 미확인 | — | — |
| mfds-drug-safety | 출처 연결, 버전 미확인 | 10 | 3194 |
| mfds-food-safety | 출처 연결, 버전 미확인 | 21 | 7217 |
| naver-blog-research | 출처 연결, 버전 미확인 | 40 | 6455 |
| naver-news-search | 출처 연결, 버전 미확인 | 15 | 3511 |
| naver-shopping-search | 미확인 | — | — |
| olive-young-search | 미확인 | — | — |
| parking-lot-search | 출처 연결, 버전 미확인 | 11 | 1703 |
| public-restroom-nearby | 출처 연결, 버전 미확인 | 11 | 2089 |
| real-estate-search | 출처 연결, 버전 미확인 | 15 | 2666 |
| rhwp-advanced | 출처 연결, 버전 미확인 | 24 | 4166 |
| rhwp-edit | 출처 연결, 버전 미확인 | 23 | 7105 |
| seoul-subway-arrival | 미확인 | — | — |
| srt-booking | 미확인 | — | — |
| subway-lost-property | 미확인 | — | — |
| toss-securities | 미확인 | — | — |
| used-car-price-search | 미확인 | — | — |
| zipcode-search | 출처 연결, 버전 미확인 | 13 | 3133 |
| kopis-openapi | 미확인 | — | — |

## 원격 저장소

| 저장소 | 커밋 또는 조회 결과 |
|---|---|
| https://github.com/challengekim/iros-registry-automation | 4502c2a33e9e10e4e05d33d83a8b162f97cd820b |
| https://github.com/comfyanonymous/comfyui | 8d534945ebd53cff61e8def81757c6a6c1b9cf2d |
| https://github.com/elder-plinius/obliteratus | b847511776a2afa7ed076f676184a4abfef2b162 |
| https://github.com/ggml-org/llama.cpp | 6c7a87f7e5e5cd75b8a641c3471f2dee84a6ed17 |
| https://github.com/hamelsmu/hamelnb | 192f491fd388bbc190bcaa95542c5d5f98dae2ab |
| https://github.com/heartmula/heartlib | a18c8cb54a55b4c24d48a2842542f81da7dedcd2 |
| https://github.com/hmmhmmhm/daiso-mcp | bec067f2611098b242f2a07e23ab93614a2b1db1 |
| https://github.com/nesquena/hermes-webui | c296673ebfaf98750fe38438bc71f0cbb1f75777 |
| https://github.com/nomadamas/k-skill | 9fade13b1066bc58fd820fe659145a9a21974138 |
| https://github.com/o/r | GitHub repository not found. |

## 검증

- 새 설치 앱을 실제 사용자 설정으로 실행한 결과, `nomadamas/k-skill` 모음에 보유 스킬 30개·파일 위치 30곳이 자동 표시됐다. 상세 화면에서 설치 안내문 경로와 본문 일치 근거도 확인했다.
- 설치 앱 실행 전후에도 조사 대상 파일의 SHA-256과 실행 권한 변경은 0개였다. 앱과 DB는 실행 전에 각각 백업했다.
- 출처 Rust 26개, DB 초기화 3개, 스킬 묶음 6개 통과. 실제 네트워크 테스트는 별도 실행.
- 기존 관련 화면/스토어 96개와 추가 근거 표시 테스트 1개 통과. 타입 검사, ESLint, 엄격한 Clippy 통과.
- 이전 임시 홈 화면 확인은 설치 항목이 0개로 재스캔되어 실제 자동 연결을 검증하지 못했다. 이번에는 실제 설치 경로를 유지한 복제 DB로 판별했다.
