export function displayFilePath(path: string | null | undefined): string {
  if (!path) return '';
  const extendedUncPrefix = '\\\\?\\UNC\\';
  if (path.startsWith(extendedUncPrefix)) return `\\\\${path.slice(extendedUncPrefix.length)}`;
  const extendedPrefix = '\\\\?\\';
  return path.startsWith(extendedPrefix) ? path.slice(extendedPrefix.length) : path;
}
