<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  type Change = { path: string; index_status: string; worktree_status: string; untracked: boolean };
  type Selection = { path: string; staged: boolean };
  let { workspace, onClose, onHistory } = $props<{ workspace: string; onClose: () => void; onHistory: () => void }>();
  let changes = $state<Change[]>([]);
  let selected = $state<Selection | null>(null);
  let diff = $state('');
  let diffLoading = $state(false);
  let loading = $state(true);
  let committing = $state(false);
  let commitMessage = $state('');
  let error = $state('');
  let notice = $state('');
  let requestId = 0;

  let stagedChanges = $derived(changes.filter(change => change.index_status !== ' ' && !change.untracked));
  let unstagedChanges = $derived(changes.filter(change => change.untracked || change.worktree_status !== ' '));
  let lines = $derived(diff.split('\n'));
  let selectedChange = $derived(selected ? changes.find(change => change.path === selected?.path) : undefined);
  let selectedHasBoth = $derived(Boolean(selectedChange && !selectedChange.untracked && selectedChange.index_status !== ' ' && selectedChange.worktree_status !== ' '));

  async function selectChange(change: Change, staged: boolean) {
    selected = { path: change.path, staged };
    await loadDiff(change.path, staged);
  }

  function selectDiffMode(staged: boolean) {
    if (!selected) return;
    const path = selected.path;
    selected = { path, staged };
    void loadDiff(path, staged);
  }

  async function loadDiff(path: string, staged: boolean) {
    const request = ++requestId;
    diffLoading = true;
    error = '';
    try {
      const result = await invoke<string>('git_file_diff', { path, staged });
      if (request === requestId) diff = result;
    } catch (reason) {
      if (request === requestId) { diff = ''; error = String(reason); }
    } finally {
      if (request === requestId) diffLoading = false;
    }
  }

  async function refresh() {
    loading = true;
    error = '';
    try {
      changes = await invoke<Change[]>('git_changes');
      const currentSelection = selected && changes.find(change => change.path === selected?.path);
      if (selected && currentSelection) {
        let staged = selected.staged;
        if (staged && (currentSelection.untracked || currentSelection.index_status === ' ')) staged = false;
        if (!staged && !currentSelection.untracked && currentSelection.worktree_status === ' ' && currentSelection.index_status !== ' ') staged = true;
        selected = { path: selected.path, staged };
        await loadDiff(selected.path, staged);
      } else if (changes.length) {
        const first = changes.find(change => change.index_status !== ' ' && !change.untracked) ?? changes[0];
        await selectChange(first, first.index_status !== ' ' && !first.untracked && first.worktree_status === ' ');
      } else {
        selected = null;
        diff = '';
      }
    } catch (reason) {
      error = String(reason);
      changes = [];
      selected = null;
      diff = '';
    } finally { loading = false; }
  }

  async function stage(change: Change) {
    error = '';
    try { await invoke('git_stage_file', { path: change.path }); await refresh(); }
    catch (reason) { error = String(reason); }
  }

  async function unstage(change: Change) {
    error = '';
    try { await invoke('git_unstage_file', { path: change.path }); await refresh(); }
    catch (reason) { error = String(reason); }
  }

  async function commit() {
    if (!commitMessage.trim() || !changes.length || committing) return;
    committing = true;
    error = '';
    notice = '';
    try {
      const hash = await invoke<string>('git_commit', { message: commitMessage });
      notice = `Committed ${hash}`;
      commitMessage = '';
      await refresh();
    } catch (reason) { error = String(reason); }
    finally { committing = false; }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') onClose();
    if ((event.ctrlKey || event.metaKey) && event.key === 'Enter' && commitMessage.trim()) {
      event.preventDefault();
      void commit();
    }
  }

  $effect(() => { if (workspace) void refresh(); });
</script>

<svelte:window onkeydown={handleKeydown} />
<div class="changes-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
  <section class="changes-dialog" role="dialog" aria-modal="true" aria-labelledby="git-changes-title">
    <header class="changes-header">
      <div class="changes-title"><span class="changes-eyebrow">GIT</span><h2 id="git-changes-title">Changes</h2><p title={workspace}>{workspace || 'No folder open'}</p></div>
      <div class="changes-actions"><button class="history-link" onclick={onHistory}>History</button><button class="changes-refresh" onclick={refresh} disabled={loading} title="Refresh changes" aria-label="Refresh changes">↻</button><button class="changes-close" onclick={onClose} aria-label="Close Git changes">×</button></div>
    </header>
    <div class="changes-body">
      <aside class="changes-sidebar">
        <div class="change-group-heading"><span>STAGED CHANGES</span><span>{stagedChanges.length}</span></div>
        {#each stagedChanges as change (`staged-${change.path}`)}
          <div class="change-entry" class:selected={selected?.path === change.path && selected?.staged}>
            <button class="change-file" onclick={() => selectChange(change, true)} title={change.path}><span class="file-status staged">{change.index_status}</span><span>{change.path}</span></button>
            <button class="stage-action" onclick={() => unstage(change)} title="Unstage file" aria-label={`Unstage ${change.path}`}>−</button>
          </div>
        {/each}
        {#if !stagedChanges.length}<p class="group-empty">No staged files</p>{/if}
        <div class="change-group-heading unstaged-heading"><span>CHANGES</span><span>{unstagedChanges.length}</span></div>
        {#each unstagedChanges as change (`unstaged-${change.path}`)}
          <div class="change-entry" class:selected={selected?.path === change.path && !selected?.staged}>
            <button class="change-file" onclick={() => selectChange(change, false)} title={change.path}><span class="file-status">{change.untracked ? 'U' : change.worktree_status}</span><span>{change.path}</span></button>
            <button class="stage-action" onclick={() => stage(change)} title="Stage file" aria-label={`Stage ${change.path}`}>＋</button>
          </div>
        {/each}
        {#if !unstagedChanges.length}<p class="group-empty">No unstaged files</p>{/if}
        {#if loading && !changes.length}<p class="group-empty">Loading changes…</p>{/if}
      </aside>
      <section class="diff-panel" aria-label="File diff">
        {#if selected}
          <div class="diff-heading"><span title={selected.path}>{selected.path}</span>{#if selectedHasBoth}<div class="diff-modes"><button class:active={!selected.staged} onclick={() => selectDiffMode(false)}>Working tree</button><button class:active={selected.staged} onclick={() => selectDiffMode(true)}>Staged</button></div>{/if}</div>
        {/if}
        <div class="diff-content">
          {#if !selected}<div class="diff-empty">{loading ? 'Loading changes…' : error || 'Your working tree is clean.'}</div>
          {:else if diffLoading}<div class="diff-empty">Loading diff…</div>
          {:else if error}<div class="diff-empty diff-error">{error}</div>
          {:else if !diff}<div class="diff-empty">No diff for this file.</div>
          {:else}<pre>{#each lines as line, index (`${index}-${line}`)}<span class:added={line.startsWith('+') && !line.startsWith('+++')} class:removed={line.startsWith('-') && !line.startsWith('---')} class:hunk={line.startsWith('@@')} class:diff-meta={line.startsWith('diff ') || line.startsWith('index ') || line.startsWith('new file') || line.startsWith('---') || line.startsWith('+++')}>{line || ' '}</span>{/each}</pre>{/if}
        </div>
      </section>
    </div>
    <footer class="commit-footer">
      <div class="commit-input-area"><textarea bind:value={commitMessage} placeholder="Commit message" aria-label="Commit message" onkeydown={(event) => { if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); void commit(); } }}></textarea><div class="commit-hint">{stagedChanges.length ? 'Only staged files will be committed.' : 'No staged files: all changes will be staged and committed.'} Ctrl+Enter to commit</div></div>
      <button class="commit-button" onclick={commit} disabled={!commitMessage.trim() || !changes.length || committing}>{committing ? 'Committing…' : stagedChanges.length ? 'Commit staged' : 'Commit all changes'}</button>
    </footer>
    {#if error || notice}<div class:notice={!error} class="changes-notice">{error || notice}</div>{/if}
  </section>
</div>

<style>
  .changes-backdrop { position:fixed; inset:0; z-index:12; display:grid; place-items:center; padding:4vh 4vw; background:#0009; }
  .changes-dialog { position:relative; width:min(1200px,94vw); height:min(900px,92vh); display:flex; flex-direction:column; overflow:hidden; color:var(--text); background:var(--panel); border:1px solid var(--line); border-radius:9px; box-shadow:0 20px 70px #0009; }
  .changes-header { flex:none; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:15px 20px; border-bottom:1px solid var(--line); }
  .changes-eyebrow { color:var(--accent); font-size:calc(var(--ui-font-size,13px) * .7); font-weight:700; letter-spacing:.16em; }
  .changes-title h2 { margin:2px 0 3px; font-size:calc(var(--ui-font-size,13px) * 1.35); font-weight:600; }
  .changes-title p { max-width:65vw; margin:0; overflow:hidden; color:var(--dim); font-size:calc(var(--ui-font-size,13px) * .82); text-overflow:ellipsis; white-space:nowrap; }
  .changes-actions { display:flex; align-items:center; gap:6px; }
  .changes-actions button { height:32px; min-width:32px; display:grid; place-items:center; padding:0 8px; border:0; border-radius:5px; background:transparent; color:var(--muted); cursor:pointer; }
  .changes-actions button:hover { background:var(--hover); color:var(--text); }
  .changes-refresh { font-size:18px; }
  .changes-close { font-size:22px; }
  .history-link { font-size:calc(var(--ui-font-size,13px) * .85); }
  .changes-body { flex:1; min-height:0; display:flex; }
  .changes-sidebar { flex:none; width:270px; overflow:auto; border-right:1px solid var(--line); padding:8px 0 12px; }
  .change-group-heading { height:30px; display:flex; align-items:center; justify-content:space-between; padding:0 12px; color:var(--muted); font-size:calc(var(--ui-font-size,13px) * .7); font-weight:700; letter-spacing:.08em; }
  .change-group-heading span:last-child { color:var(--dim); font-weight:500; }
  .unstaged-heading { margin-top:8px; border-top:1px solid var(--line); height:38px; }
  .change-entry { height:31px; display:flex; align-items:center; padding:0 6px 0 10px; gap:4px; }
  .change-entry.selected { background:var(--select); }
  .change-file { min-width:0; flex:1; height:100%; display:flex; align-items:center; gap:7px; overflow:hidden; border:0; background:transparent; color:var(--text); text-align:left; cursor:pointer; font-size:calc(var(--ui-font-size,13px) * .82); }
  .change-file span:last-child { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .file-status { flex:none; width:13px; color:#d6a065; text-align:center; font:600 calc(var(--ui-font-size,13px) * .75) var(--mono); }
  .file-status.staged { color:#7fb6a7; }
  .stage-action { flex:none; width:24px; height:24px; border:0; border-radius:4px; background:transparent; color:var(--muted); cursor:pointer; font-size:17px; line-height:1; }
  .stage-action:hover { background:var(--hover); color:var(--text); }
  .group-empty { margin:0; padding:4px 12px 8px; color:var(--dim); font-size:calc(var(--ui-font-size,13px) * .77); }
  .diff-panel { min-width:0; flex:1; display:flex; flex-direction:column; }
  .diff-heading { height:40px; flex:none; display:flex; align-items:center; justify-content:space-between; gap:12px; padding:0 14px; border-bottom:1px solid var(--line); color:var(--muted); font-size:calc(var(--ui-font-size,13px) * .82); }
  .diff-heading > span { min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
  .diff-modes { flex:none; display:flex; gap:3px; }
  .diff-modes button { border:0; border-radius:4px; padding:5px 8px; background:transparent; color:var(--dim); cursor:pointer; font-size:calc(var(--ui-font-size,13px) * .75); }
  .diff-modes button.active,.diff-modes button:hover { background:var(--hover); color:var(--text); }
  .diff-content { flex:1; min-height:0; overflow:auto; background:var(--bg); }
  .diff-content pre { min-width:max-content; margin:0; padding:10px 0 20px; color:var(--text); font:calc(var(--ui-font-size,13px) * .82)/1.55 var(--mono); }
  .diff-content pre span { display:block; min-height:1.55em; padding:0 14px; white-space:pre; }
  .diff-content pre span.added { background:#1c382c; color:#9bd4b2; }
  .diff-content pre span.removed { background:#43282b; color:#e6a3a3; }
  .diff-content pre span.hunk { margin:4px 0; background:#202d3a; color:#91b7d8; }
  .diff-content pre span.diff-meta { color:var(--dim); }
  .diff-empty { height:100%; display:grid; place-items:center; padding:20px; color:var(--dim); font-size:var(--ui-font-size,13px); text-align:center; white-space:pre-wrap; }
  .diff-error { color:#e39b91; }
  .commit-footer { flex:none; display:flex; align-items:stretch; gap:12px; padding:12px 14px; border-top:1px solid var(--line); }
  .commit-input-area { min-width:0; flex:1; display:flex; flex-direction:column; gap:5px; }
  .commit-input-area textarea { width:100%; min-height:54px; max-height:110px; resize:vertical; border:1px solid var(--line); border-radius:5px; padding:8px 10px; outline:0; background:var(--bg); color:var(--text); font:var(--ui-font-size,13px) var(--mono); }
  .commit-input-area textarea:focus { border-color:var(--accent); }
  .commit-hint { color:var(--dim); font-size:calc(var(--ui-font-size,13px) * .72); }
  .commit-button { flex:none; align-self:center; min-height:36px; border:1px solid var(--accent); border-radius:5px; padding:0 14px; background:var(--accent); color:#17191c; cursor:pointer; font-size:calc(var(--ui-font-size,13px) * .85); font-weight:600; }
  .commit-button:disabled { opacity:.45; cursor:default; }
  .changes-notice { position:absolute; right:16px; bottom:126px; max-width:min(440px,70vw); padding:8px 11px; border:1px solid var(--line); border-radius:5px; background:var(--panel-2); box-shadow:0 4px 16px #0005; color:#e39b91; font-size:calc(var(--ui-font-size,13px) * .82); overflow-wrap:anywhere; }
  .changes-notice.notice { color:#9bd4b2; }
  @media(max-width:700px) { .changes-backdrop{padding:2vh 2vw}.changes-dialog{width:96vw;height:96vh}.changes-sidebar{width:40%}.commit-footer{flex-direction:column}.commit-button{align-self:flex-end}.changes-title p{max-width:45vw} }
</style>
