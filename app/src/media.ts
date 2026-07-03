import { convertFileSrc } from "@tauri-apps/api/core";

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/");
}

export function joinProjectPath(projectPath: string, relativePath: string): string {
  const base = normalizePath(projectPath).replace(/\/$/, "");
  const relative = normalizePath(relativePath).replace(/^\//, "");
  return `${base}/${relative}`;
}

export function projectMediaUrl(projectPath: string, relativePath: string): string {
  return convertFileSrc(joinProjectPath(projectPath, relativePath));
}

export function mediaFileUrl(absolutePath: string): string {
  return convertFileSrc(normalizePath(absolutePath));
}