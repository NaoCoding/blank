<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { EditorState, StateEffect, type Extension } from '@codemirror/state';
  import { EditorView, lineNumbers, highlightActiveLine, keymap, highlightSpecialChars, drawSelection, rectangularSelection, crosshairCursor } from '@codemirror/view';
  import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
  import { bracketMatching, indentOnInput, syntaxHighlighting, defaultHighlightStyle, StreamLanguage } from '@codemirror/language';
  import type { Document } from '../lib/documents';

  let { doc, dark, fontSize, getState, onUpdate, onStatus } = $props<{
    doc: Document; dark: boolean; fontSize: number; getState: () => EditorState | undefined;
    onUpdate: (content: string, state: EditorState) => void; onStatus: (line: number, column: number) => void;
  }>();
  let host: HTMLDivElement;
  let view: EditorView | undefined;
  let activeId = '';
  const theme = (isDark: boolean, size: number): Extension => EditorView.theme({
    '&': { height: '100%', color: isDark ? '#d7d9dc' : '#26282b', backgroundColor: isDark ? '#17191c' : '#ffffff' },
    '.cm-content': { fontFamily: 'var(--mono)', fontSize: `${size}px`, caretColor: isDark ? '#d6a76c' : '#98682d', padding: '14px 0' },
    '.cm-gutters': { backgroundColor: isDark ? '#17191c' : '#fafafa', color: isDark ? '#686d75' : '#9a9da2', border: 'none' },
    '.cm-activeLine': { backgroundColor: isDark ? '#1d2024' : '#f7f8fa' },
    '.cm-activeLineGutter': { color: isDark ? '#c6c8cc' : '#555b63' },
    '.cm-selectionBackground': { backgroundColor: isDark ? '#374251' : '#d9e5f4' },
    '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground': { backgroundColor: isDark ? '#465a72' : '#b7d0ee' },
    '::selection': { backgroundColor: isDark ? '#465a72' : '#b7d0ee' },
    '.cm-cursor': { borderLeftColor: isDark ? '#e2e3e6' : '#25282c' },
    '.cm-scroller': { overflow: 'auto', fontFamily: 'var(--mono)' }
  }, { dark: isDark });
  const langExtension = async (name: string): Promise<Extension> => {
    switch (name) {
      case 'javascript': return (await import('@codemirror/lang-javascript')).javascript();
      case 'typescript': return (await import('@codemirror/lang-javascript')).javascript({ typescript: true });
      case 'python': return (await import('@codemirror/lang-python')).python();
      case 'json': return (await import('@codemirror/lang-json')).json();
      case 'html': return (await import('@codemirror/lang-html')).html();
      case 'css': return (await import('@codemirror/lang-css')).css();
      case 'cpp': return (await import('@codemirror/lang-cpp')).cpp();
      case 'rust': return (await import('@codemirror/lang-rust')).rust();
      case 'yaml': return (await import('@codemirror/lang-yaml')).yaml();
      case 'markdown': return (await import('@codemirror/lang-markdown')).markdown();
      case 'java': return (await import('@codemirror/lang-java')).java();
      case 'latex': { const { stex } = await import('@codemirror/legacy-modes/mode/stex'); return StreamLanguage.define(stex); }
      default: return [];
    }
  };
  function createState(): EditorState {
    return getState() ?? EditorState.create({ doc: doc.content, extensions: [
      lineNumbers(), highlightActiveLine(), highlightSpecialChars(), drawSelection(), history(),
      indentOnInput(), bracketMatching(), rectangularSelection(), crosshairCursor(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }), theme(dark, fontSize),
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      EditorView.updateListener.of(update => {
        if (update.docChanged) onUpdate(update.state.doc.toString(), update.state);
        if (update.selectionSet || update.docChanged) { const head = update.state.selection.main.head; const line = update.state.doc.lineAt(head); onStatus(line.number, head - line.from + 1); }
      })
    ] });
  }
  onMount(() => { view = new EditorView({ state: createState(), parent: host }); activeId = doc.id; });
  $effect(() => {
    if (!view) return;
    if (doc.id !== activeId) { const next = getState() ?? EditorState.create({ doc: doc.content, extensions: [
      lineNumbers(), highlightActiveLine(), highlightSpecialChars(), drawSelection(), history(), indentOnInput(), bracketMatching(),
      syntaxHighlighting(defaultHighlightStyle, { fallback: true }), theme(dark, fontSize),
      keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]),
      EditorView.updateListener.of(update => { if (update.docChanged) onUpdate(update.state.doc.toString(), update.state); if (update.selectionSet || update.docChanged) { const p=update.state.selection.main.head,l=update.state.doc.lineAt(p);onStatus(l.number,p-l.from+1); } })
    ] }); view.setState(next); activeId = doc.id; }
  });
  $effect(() => {
    if (!view || doc.id !== activeId) return;
    const current = view, language = doc.language, isDark = dark, size = fontSize;
    let cancelled = false;
    void langExtension(language).then(lang => {
      if (cancelled || view !== current) return;
      current.dispatch({ effects: StateEffect.reconfigure.of([lineNumbers(), highlightActiveLine(), highlightSpecialChars(), drawSelection(), history(), indentOnInput(), bracketMatching(), syntaxHighlighting(defaultHighlightStyle, { fallback: true }), lang, theme(isDark, size), keymap.of([...defaultKeymap, ...historyKeymap, indentWithTab]), EditorView.updateListener.of(update => { if(update.docChanged) onUpdate(update.state.doc.toString(),update.state); if(update.selectionSet || update.docChanged){const p=update.state.selection.main.head,l=update.state.doc.lineAt(p);onStatus(l.number,p-l.from+1);} })]) });
    });
    return () => { cancelled = true; };
  });
  onDestroy(() => view?.destroy());
</script>
<div class="editor-host" bind:this={host}></div>
