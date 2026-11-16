import i18n from "@/i18n";

/** 백엔드가 `코드: 영어 설명`으로 보내는 오류 코드와 화면 문구 키. */
const BACKEND_ERROR_TEXT_KEYS: Record<string, string> = {
  GITHUB_MIRROR_DISABLED: "marketplace.githubMirrorDisabledError",
  GITHUB_ARCHIVE_DOWNLOAD_LIMIT: "marketplace.githubArchiveDownloadLimitError",
  GITHUB_ARCHIVE_EXTRACTION_LIMIT: "marketplace.githubArchiveExtractionLimitError",
};

/** 코드가 붙은 백엔드 오류는 화면 언어 문구로, 그 밖의 오류는 그대로 문자열로 돌려준다. */
export function localizeBackendError(error: unknown): string {
  const message = String(error);
  const code = Object.keys(BACKEND_ERROR_TEXT_KEYS).find((candidate) =>
    message.includes(`${candidate}:`),
  );
  return code ? i18n.t(BACKEND_ERROR_TEXT_KEYS[code]) : message;
}
