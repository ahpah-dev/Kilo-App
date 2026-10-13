import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { kiloApi } from '@/services/kiloApi';

/** Show a workspace-relative path in the system file explorer. */
export async function revealInExplorer(relativePath: string): Promise<void> {
  try {
    const absolute = await kiloApi.resolveWorkspacePath(relativePath);
    await revealItemInDir(absolute);
  } catch (e) {
    console.warn('Could not reveal path', relativePath, e);
  }
}

export function baseName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function dirName(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  parts.pop();
  return parts.join('/');
}
