import { describe, expect, it } from 'vitest';
import { createDocument, isDirty, languageFromPath, saveDocument } from '../src/lib/documents';
import { EditorState } from '@codemirror/state';
import { history, undo } from '@codemirror/commands';

describe('document state', () => {
  it('maps supported file extensions to editor modes', () => {
    expect(languageFromPath('readme.MD')).toBe('markdown');
    expect(languageFromPath('app.ts')).toBe('typescript');
    expect(languageFromPath('main.rs')).toBe('rust');
    expect(languageFromPath('paper.tex')).toBe('latex');
    expect(languageFromPath('unknown.bin')).toBe('plaintext');
  });
  it('tracks dirty state and records saves', () => {
    const doc = createDocument('C:\\project\\notes.md', 'draft');
    expect(isDirty(doc)).toBe(false);
    const changed = { ...doc, content: 'edited' };
    expect(isDirty(changed)).toBe(true);
    expect(isDirty(saveDocument(changed))).toBe(false);
  });
  it('keeps untitled documents distinct', () => {
    expect(createDocument(null).id).not.toBe(createDocument(null).id);
  });
  it('retains editor history when switching between stored document states', () => {
    const views = new Map<string, EditorState>();
    const first = EditorState.create({ doc: 'first', extensions: [history()] });
    const edited = first.update({ changes: { from: 5, insert: '!' } }).state;
    views.set('first', edited);
    views.set('second', EditorState.create({ doc: 'second', extensions: [history()] }));
    const restored = views.get('first')!;
    expect(restored.doc.toString()).toBe('first!');
    let undone = restored;
    const didUndo = undo({ state: restored, dispatch: transaction => { undone = transaction.state; } });
    expect(didUndo).toBe(true);
    expect(undone.doc.toString()).toBe('first');
  });
});
