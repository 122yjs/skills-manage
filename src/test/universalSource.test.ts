import { describe, expect, it } from "vitest";
import { isUniversalSource } from "@/lib/agents";
import { ScannedSkill } from "@/types";

const universalRoot = "~/.agents/skills";

function skill(overrides: Partial<ScannedSkill> = {}): ScannedSkill {
  return {
    id: "tdd",
    name: "tdd",
    file_path: "~/.agents/skills/tdd/SKILL.md",
    dir_path: "~/.agents/skills/tdd",
    link_type: "symlink",
    is_central: false,
    source_kind: "compatibility",
    source_root: universalRoot,
    is_read_only: true,
    ...overrides,
  };
}

// compatibility는 "다른 경로에서 읽었다"는 뜻일 뿐이라 공용 설치와 같지 않다.
// 출처 경로가 실제 공용 설치 경로와 같을 때만 공용 설치로 인정한다.
describe("isUniversalSource", () => {
  it("공용 경로에서 읽은 호환 출처는 공용 설치로 본다", () => {
    expect(isUniversalSource(skill(), universalRoot)).toBe(true);
  });

  it("경로 표기 차이(구분자·끝 슬래시)는 같은 경로로 본다", () => {
    expect(
      isUniversalSource(
        skill({ source_root: "~\\.agents\\skills\\" }),
        `${universalRoot}/`
      )
    ).toBe(true);
  });

  const notUniversal: Array<[string, Partial<ScannedSkill>]> = [
    ["다른 플랫폼 전용 경로", { source_root: "~/.codex/skills" }],
    ["출처 경로 없음", { source_root: null }],
    ["호환 출처가 아님(user)", { source_kind: "user" }],
    ["호환 출처가 아님(plugin)", { source_kind: "plugin" }],
    ["읽기 전용이 아님", { is_read_only: false }],
  ];

  it.each(notUniversal)("%s 항목은 공용 설치로 보지 않는다", (_label, overrides) => {
    expect(isUniversalSource(skill(overrides), universalRoot)).toBe(false);
  });

  it("공용 설치 경로를 알 수 없으면 공용으로 추정하지 않는다", () => {
    expect(isUniversalSource(skill(), undefined)).toBe(false);
    expect(isUniversalSource(skill(), "")).toBe(false);
  });
});
