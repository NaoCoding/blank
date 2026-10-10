<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { open, save } from '@tauri-apps/plugin-dialog';
  import { EditorState } from '@codemirror/state';
  import EditorPane from './components/EditorPane.svelte';
  import FileTree from './components/FileTree.svelte';
  import { createDocument, isDirty, saveDocument, type Document } from './lib/documents';

  let workspace = $state(''); let documents = $state<Document[]>([]); let activeId = $state('');
  let editorStates = new Map<string, EditorState>(); let sidebarOpen = $state(true); let terminalOpen = $state(false);
  let previewOpen = $state(true); let dark = $state(true); let split = $state(52); let sidebarWidth = $state(232); let terminalHeight = $state(220);
  let previewHtml = $state(''); let line = $state(1); let column = $state(1); let toast = $state(''); let showQuickOpen = $state(false); let quickQuery = $state('');
  let TerminalPaneComponent = $state<any>(null);
  let recentFiles = $state<string[]>([]); let previewTimer: ReturnType<typeof setTimeout>;
  const appWindow = getCurrentWindow();
  let maximized = $state(false);
  const active = $derived(documents.find(d => d.id === activeId));
  const dirtyCount = $derived(documents.filter(isDirty).length);
  const markdown = $derived(active?.language === 'markdown');
  const displayWorkspace = $derived(workspace.split(/[\\/]/).filter(Boolean).at(-1) ?? 'No folder');

  function notify(message: string) { toast = message; setTimeout(() => { if (toast === message) toast = ''; }, 3000); }
  async function chooseWorkspace() {
    const path = await open({ directory: true, multiple: false, title: 'Open Project Folder' });
    if (typeof path !== 'string') return;
    if (dirtyCount && !confirm('Discard unsaved changes before opening another folder?')) return;
    try {
      const nextWorkspace = await invoke<string>('set_workspace', { path });
      if (nextWorkspace !== workspace) { documents = []; editorStates.clear(); activeId = ''; terminalOpen = false; }
      workspace = nextWorkspace; notify(`Opened ${workspace}`);
    }
    catch (e) { notify(String(e)); }
  }
  async function openFile(path: string) {
    const existing = documents.find(d => d.path === path); if (existing) { activeId = existing.id; return; }
    try {
      const snapshot = await invoke<{ content: string; version: string }>('read_file', { path });
      const doc = { ...createDocument(path, snapshot.content), version: snapshot.version }; documents = [...documents, doc]; activeId = doc.id; recentFiles = [path, ...recentFiles.filter(p => p !== path)].slice(0, 40);
    } catch (e) { notify(`Could not open file: ${e}`); }
  }
  async function chooseFile() {
    try {
      const path = await open({ multiple: false, directory: false, title: 'Open File' });
      if (typeof path !== 'string') return;
      const target = await invoke<{ path: string; workspace: string; needs_workspace: boolean }>('resolve_open_file', { path });
      if (target.needs_workspace) {
        if (dirtyCount && !confirm('Discard unsaved changes before opening a file from another folder?')) return;
        workspace = await invoke<string>('set_workspace', { path: target.workspace });
        documents = []; editorStates.clear(); activeId = ''; terminalOpen = false;
      }
      await openFile(target.path);
    } catch (e) { notify(`Could not open file: ${e}`); }
  }
  function newFile() { const doc = createDocument(null); documents = [...documents, doc]; activeId = doc.id; }
  async function saveActive(saveAs = false) {
    if (!active) return;
    let path: string | null = active.path;
    if (!path || saveAs) {
      path = await save({ title: 'Save File', defaultPath: [workspace, active.name].filter(Boolean).join('\\'), filters: [{ name: 'Text', extensions: ['*'] }] });
      if (!path) return;
    }
    try {
      const version = active.path === path
        ? await invoke<string>('write_file', { path, contents: active.content, expectedVersion: active.version })
        : await invoke<string>('create_file', { path, contents: active.content });
      const saved = saveDocument(active, path, version);
      documents = documents.map(d => d.id === active.id ? saved : d); activeId = saved.id; notify('Saved');
    } catch (e) { notify(`Could not save file: ${e}`); }
  }
  function updateDoc(id: string, content: string, state: EditorState) {
    editorStates.set(id, state);
    documents = documents.map(d => d.id === id ? { ...d, content, cursor: state.selection.main.head } : d);
  }
  function closeTab(id: string) {
    const doc = documents.find(d => d.id === id); if (!doc) return;
    if (isDirty(doc) && !confirm(`Discard unsaved changes to ${doc.name}?`)) return;
    editorStates.delete(id); documents = documents.filter(d => d.id !== id);
    if (activeId === id) activeId = documents.at(-1)?.id ?? '';
  }
  async function createNewFromDialog() {
    if (!workspace) { await chooseWorkspace(); if (!workspace) return; }
    const path = await save({ title: 'Create New File', defaultPath: workspace });
    if (!path) return;
    try { await invoke('create_file', { path, contents: '' }); await openFile(path); }
    catch (e) { notify(`Could not create file: ${e}`); }
  }
  let previewRevision = 0;
  function updatePreview(value: string) {
    clearTimeout(previewTimer); const revision = ++previewRevision;
    previewTimer = setTimeout(() => { void import('./lib/markdown').then(({ renderMarkdown }) => { if (revision === previewRevision) previewHtml = renderMarkdown(value); }); }, 120);
  }
  $effect(() => { if (markdown && previewOpen) updatePreview(active?.content ?? ''); });
  async function toggleTerminal() {
    if (!TerminalPaneComponent) TerminalPaneComponent = (await import('./components/TerminalPane.svelte')).default;
    terminalOpen = !terminalOpen;
  }
  async function toggleWindowMaximize() {
    await appWindow.toggleMaximize();
    maximized = await appWindow.isMaximized();
  }
  function handleTitlebarPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    if (event.target instanceof Element && event.target.closest('button, input, textarea, select, .tab')) return;
    event.preventDefault();
    if (event.detail === 2) void toggleWindowMaximize();
    else void appWindow.startDragging();
  }

  function startResize(event: PointerEvent, target: 'sidebar' | 'split' | 'terminal') {
    event.preventDefault(); const startX = event.clientX, startY = event.clientY;
    const initial = target === 'sidebar' ? sidebarWidth : target === 'split' ? split : terminalHeight;
    const move = (e: PointerEvent) => {
      if (target === 'sidebar') sidebarWidth = Math.min(360, Math.max(170, initial + e.clientX - startX));
      else if (target === 'split') { const area = document.querySelector('.work-area')?.clientWidth ?? 1; split = Math.min(78, Math.max(22, initial + (e.clientX - startX) / area * 100)); }
      else terminalHeight = Math.min(480, Math.max(120, initial - (e.clientY - startY)));
    };
    const done = () => { window.removeEventListener('pointermove', move); window.removeEventListener('pointerup', done); };
    window.addEventListener('pointermove', move); window.addEventListener('pointerup', done, { once: true });
  }
  function togglePreview() { if (markdown) previewOpen = !previewOpen; else notify('Preview is available for Markdown files'); }
  function keydown(event: KeyboardEvent) {
    const mod = event.ctrlKey || event.metaKey;
    if (mod && event.key.toLowerCase() === 's') { event.preventDefault(); void saveActive(event.shiftKey); }
    else if (mod && event.key.toLowerCase() === 'o' && event.shiftKey) { event.preventDefault(); void chooseWorkspace(); }
    else if (mod && event.key.toLowerCase() === 'o') { event.preventDefault(); void chooseFile(); }
    else if (mod && event.key.toLowerCase() === 'p' && event.shiftKey) { event.preventDefault(); togglePreview(); }
    else if (mod && event.key.toLowerCase() === 'p') { event.preventDefault(); showQuickOpen = true; quickQuery = ''; }
    else if (mod && event.key.toLowerCase() === 'n') { event.preventDefault(); newFile(); }
    else if (mod && event.key.toLowerCase() === 'w') { event.preventDefault(); if (active) closeTab(active.id); }
    else if (mod && event.key.toLowerCase() === 'b') { event.preventDefault(); sidebarOpen = !sidebarOpen; }
    else if (event.ctrlKey && event.key === '`') { event.preventDefault(); void toggleTerminal(); }
    else if (event.key === 'Escape' && showQuickOpen) showQuickOpen = false;
  }
  onMount(() => {
    dark = localStorage.getItem('blank-theme') !== 'light';
    void appWindow.isMaximized().then(value => { maximized = value; });
    let unlistenResize: (() => void) | undefined;
    void appWindow.onResized(async () => { maximized = await appWindow.isMaximized(); }).then(unlisten => { unlistenResize = unlisten; });
    const handle = (e: BeforeUnloadEvent) => { if (dirtyCount) { e.preventDefault(); e.returnValue = ''; } };
    window.addEventListener('beforeunload', handle); return () => { unlistenResize?.(); window.removeEventListener('beforeunload', handle); clearTimeout(previewTimer); };
  });
</script>

<svelte:window onkeydown={keydown} />
<div class:light={!dark} class="app-shell">
  <div class="main-area">
    {#if sidebarOpen}<aside class="sidebar" style={`width:${sidebarWidth}px`}>
      <div class="side-heading"><span>EXPLORER</span><button class="icon-button" onclick={createNewFromDialog} title="New file">＋</button></div>
      {#if workspace}<div class="workspace-label" title={workspace}>{displayWorkspace.toUpperCase()}</div><FileTree path={workspace} onOpen={openFile} />{:else}<div class="empty-sidebar"><span class="empty-glyph">⌂</span><p>Open a folder to browse your files.</p><button class="subtle-button" onclick={chooseWorkspace}>Open Folder</button></div>{/if}
    </aside><div class="resize-bar vertical" role="separator" onpointerdown={(e) => startResize(e, 'sidebar')}></div>{/if}
    <main class="center-column">
      <div class="tabs-row" onpointerdown={handleTitlebarPointerDown}>
        <div class="workspace-actions">
          <div class="brand"><span class="brand-mark" data-tauri-drag-region>b</span><span data-tauri-drag-region>blank</span></div>
          <button onclick={chooseWorkspace} title="Open folder (Ctrl+Shift+O)">Open folder</button>
          <button onclick={chooseFile} title="Open file (Ctrl+O)">Open file</button>
        </div>
        <div class="tabs" data-tauri-drag-region>{#each documents as doc (doc.id)}<div class:active={doc.id === activeId} class="tab" title={doc.path ?? doc.name}><button class="tab-open" onclick={() => activeId = doc.id}><span class="tab-dot" class:changed={isDirty(doc)}>{isDirty(doc) ? '●' : '◈'}</span><span>{doc.name}</span></button><button class="close-tab" aria-label={`Close ${doc.name}`} onclick={() => closeTab(doc.id)}>×</button></div>{/each}</div>
        <div class="workspace-tools">
          <button class="toolbar-button" onclick={() => { dark = !dark; localStorage.setItem('blank-theme', dark ? 'dark' : 'light'); }} title="Toggle theme">{dark ? '☼' : '☾'}</button>
          <button class="toolbar-button" onclick={toggleTerminal} title="Toggle terminal">⌘</button>
        </div>
        {#if markdown}<button class:pressed={previewOpen} class="toolbar-button preview-toggle" onclick={togglePreview}>◫ <span>Preview</span></button>{/if}
        <div class="window-controls">
          <button aria-label="Minimize" title="Minimize" onclick={() => appWindow.minimize()}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="M1.5 6h9" /></svg></button>
          <button aria-label={maximized ? 'Restore' : 'Maximize'} title={maximized ? 'Restore' : 'Maximize'} onclick={toggleWindowMaximize}>
            {#if maximized}<svg viewBox="0 0 12 12" aria-hidden="true"><path d="M4 2.5h5.5V8M8 4.5H2.5V10H8V4.5Z" /></svg>{:else}<svg viewBox="0 0 12 12" aria-hidden="true"><rect x="1.5" y="1.5" width="9" height="9" /></svg>{/if}
          </button>
          <button class="close-window" aria-label="Close" title="Close" onclick={() => appWindow.close()}><svg viewBox="0 0 12 12" aria-hidden="true"><path d="m3 3 6 6M9 3 3 9" /></svg></button>
        </div>
      </div>
      <div class="work-area" class:with-preview={markdown && previewOpen} style={`--split:${split}%`}>
        {#if active}<section class="editor-pane"><EditorPane doc={active} dark={dark} getState={() => editorStates.get(active.id)} onUpdate={(content,state) => updateDoc(active.id,content,state)} onStatus={(l,c) => { line=l;column=c; }} /></section>
          {#if markdown && previewOpen}<div class="resize-bar vertical split-resize" role="separator" onpointerdown={(e) => startResize(e, 'split')}></div><section class="preview-pane"><div class="preview-label">PREVIEW</div><article class="markdown-body">{@html previewHtml}</article></section>{/if}
        {:else}<section class="welcome"><div class="welcome-content"><div class="welcome-icon">b</div><h1>blank</h1><p>A quiet place to work with text, code, and ideas.</p><div class="welcome-buttons"><button class="primary-button" onclick={chooseWorkspace}>Open Folder</button><button class="subtle-button" onclick={chooseFile}>Open File</button><button class="subtle-button" onclick={newFile}>New File</button></div><div class="shortcut-hint"><kbd>Ctrl</kbd><span>+</span><kbd>P</kbd><span>Quick file open</span></div></div></section>{/if}
      </div>
      {#if terminalOpen}<div class="resize-bar horizontal" role="separator" onpointerdown={(e) => startResize(e, 'terminal')}></div><section class="terminal-panel" style={`height:${terminalHeight}px`}><div class="terminal-heading"><span>TERMINAL</span><span class="terminal-cwd">{workspace || 'Choose a folder first'}</span><button class="icon-button" onclick={() => terminalOpen = false} aria-label="Close terminal">×</button></div>{#if workspace && TerminalPaneComponent}<TerminalPaneComponent cwd={workspace} dark={dark} />{:else}<div class="terminal-empty">Open a folder before starting a terminal.</div>{/if}</section>{/if}
    </main>
  </div>
  <footer class="statusbar"><div class="status-left"><span class="status-icon">⌘</span><span>{displayWorkspace}</span>{#if dirtyCount}<span>{dirtyCount} unsaved</span>{/if}</div><div class="status-right">{#if active}<span>Ln {line}, Col {column}</span><span>{active.language === 'markdown' ? 'Markdown' : active.language === 'plaintext' ? 'Plain Text' : active.language.toUpperCase()}</span><span>UTF-8</span>{/if}<button class="status-theme" onclick={() => { dark = !dark; localStorage.setItem('blank-theme',dark?'dark':'light'); }}>{dark ? 'Dark' : 'Light'}</button></div></footer>
  {#if toast}<div class="toast">{toast}</div>{/if}
  {#if showQuickOpen}<div class="modal-backdrop"><div class="quick-modal"><input bind:value={quickQuery} placeholder="Search open and recent files…" onkeydown={(e) => { if(e.key==='Enter'){const p=recentFiles.find(x=>x.toLowerCase().includes(quickQuery.toLowerCase()));if(p)void openFile(p);showQuickOpen=false;} }} /><div class="quick-list">{#each recentFiles.filter(p => p.toLowerCase().includes(quickQuery.toLowerCase())).slice(0,10) as p}<button onclick={() => { void openFile(p);showQuickOpen=false; }}><span>◈</span>{p.split(/[\\/]/).at(-1)}<small>{p}</small></button>{/each}{#if !recentFiles.length}<p>Open a file first to see it here.</p>{/if}</div></div></div>{/if}
</div>
