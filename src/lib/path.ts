export function normalizePathSeparators(path: string): string {
  return path.replace(/\\/g, "/");
}

export function isWindowsPath(path: string): boolean {
  const normalized = normalizePathSeparators(path);
  return /^[A-Za-z]:\//.test(normalized) || normalized.startsWith("//") || path.includes("\\");
}

export function formatPathForDisplay(path: string): string {
  if (!path) {
    return path;
  }
  return isWindowsPath(path) ? path.replace(/\//g, "\\") : path;
}

export function compactHomePath(path: string): string {
  const normalized = normalizePathSeparators(path);
  if (normalized === "~") {
    return "~";
  }
  if (normalized.startsWith("~/")) {
    return normalized;
  }

  const homePatterns = [
    /^\/Users\/[^/]+\/?(.*)$/,
    /^\/home\/[^/]+\/?(.*)$/,
    /^[A-Za-z]:\/Users\/[^/]+\/?(.*)$/,
  ];

  for (const pattern of homePatterns) {
    const match = normalized.match(pattern);
    if (!match) {
      continue;
    }

    const rest = match[1];
    return rest ? `~/${rest}` : "~";
  }

  return normalized;
}

export function deriveHomeDir(path: string): string | undefined {
  const normalized = normalizePathSeparators(path).replace(/\/+$/, "");
  if (normalized === "~" || normalized.startsWith("~/")) {
    return "~";
  }

  const homePatterns = [
    /^((?:\/Users|\/home)\/[^/]+)(?:\/.*)?$/,
    /^([A-Za-z]:\/Users\/[^/]+)(?:\/.*)?$/,
    /^(\/\/[^/]+\/[^/]+)(?:\/.*)?$/,
  ];

  for (const pattern of homePatterns) {
    const match = normalized.match(pattern);
    if (match?.[1]) {
      return formatPathForDisplay(match[1]);
    }
  }

  return undefined;
}

export function joinPathForDisplay(basePath: string, relativePath: string): string {
  const normalizedBase = normalizePathSeparators(basePath).replace(/\/+$/, "");
  const normalizedRelative = normalizePathSeparators(relativePath).replace(/^\/+/, "");

  if (!normalizedBase) {
    return formatPathForDisplay(normalizedRelative);
  }

  return formatPathForDisplay(`${normalizedBase}/${normalizedRelative}`);
}

export function getPathBasename(path: string): string | undefined {
  const normalized = normalizePathSeparators(path).replace(/\/+$/, "");
  if (!normalized || normalized === "~") {
    return undefined;
  }

  const parts = normalized.split("/");
  return parts[parts.length - 1] || undefined;
}

export function describeSkillsPattern(path: string): string {
  const compact = compactHomePath(path);
  return compact.startsWith("~/") ? compact.slice(2) : compact;
}

/** 심볼릭 링크 대상이 상대 경로면 링크가 있는 폴더를 기준으로 풀어 쓴다. */
export function resolveSymlinkTarget(linkPath: string, target: string): string {
  const normalizedTarget = normalizePathSeparators(target);
  const isAbsolute = normalizedTarget.startsWith("/") || normalizedTarget.startsWith("~") || isWindowsPath(target);
  const joined = isAbsolute
    ? normalizedTarget
    : `${normalizePathSeparators(linkPath).replace(/\/+$/, "")}/../${normalizedTarget}`;
  const [root, ...rest] = joined.split("/");
  const parts: string[] = [];
  for (const part of rest) {
    if (part === "..") parts.pop();
    else if (part && part !== ".") parts.push(part);
  }
  return [root, ...parts].join("/");
}

/** `path`가 `root` 폴더 자신이거나 그 안에 있으면 참이다. */
export function isPathInside(path: string, root: string): boolean {
  const normalizedPath = normalizePathSeparators(path).replace(/\/+$/, "");
  const normalizedRoot = normalizePathSeparators(root).replace(/\/+$/, "");
  return normalizedPath === normalizedRoot || normalizedPath.startsWith(`${normalizedRoot}/`);
}
