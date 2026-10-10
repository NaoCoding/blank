export type Document = {
  id: string; path: string | null; name: string; content: string; saved: string;
  language: string; cursor: number; scrollTop: number; version: string | null;
};

export const isDirty = (doc: Pick<Document, 'content' | 'saved'>) => doc.content !== doc.saved;

export function languageFromPath(path: string): string {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  const names: Record<string, string> = {
    md: 'markdown', markdown: 'markdown', mdown: 'markdown', txt: 'plaintext', log: 'plaintext',
    js: 'javascript', jsx: 'javascript', mjs: 'javascript', cjs: 'javascript', ts: 'typescript', tsx: 'typescript',
    py: 'python', json: 'json', html: 'html', htm: 'html', css: 'css', scss: 'css', c: 'cpp', h: 'cpp', cc: 'cpp', cpp: 'cpp', hpp: 'cpp',
    rs: 'rust', yml: 'yaml', yaml: 'yaml', tex: 'latex', latex: 'latex', java: 'java'
  };
  return names[ext] ?? 'plaintext';
}

export function createDocument(path: string | null, content = ''): Document {
  const name = path?.split(/[\\/]/).pop() || 'Untitled';
  return { id: path ?? `untitled-${crypto.randomUUID()}`, path, name, content, saved: content,
    language: path ? languageFromPath(path) : 'plaintext', cursor: 0, scrollTop: 0, version: null };
}

export function saveDocument(doc: Document, path = doc.path, version = doc.version): Document {
  return { ...doc, path, id: path ?? doc.id, name: path?.split(/[\\/]/).pop() ?? doc.name,
    language: path ? languageFromPath(path) : doc.language, saved: doc.content, version };
}
