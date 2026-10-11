<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  type Worktree = { path: string; head: string; branch: string | null; detached: boolean; current: boolean };
  type Worktrees = { repository: string; worktrees: Worktree[] };

  let workspace = $props<string>();
  let result = $state<Worktrees | null>(null);
  let error = $state('');
  let loading = $state(false);

  async function refresh() {
    if (!workspace) return;
    loading = true;
    error = '';
    try {
      result = await invoke<Worktrees>('git_worktrees');
    } catch (reason) {
      result = null;
      error = String(reason);
    } finally {
      loading = false;
    }
  }

  $effect(() => {
    if (workspace) void refresh();
  });
</script>

<section class="worktrees" aria-label="Git worktrees">
  <header class="worktrees-heading">
    <span>GIT WORKTREES</span>
    <button class="refresh-button" onclick={refresh} disabled={loading || !workspace} title="Refresh worktrees" aria-label="Refresh worktrees">↻</button>
  </header>
  {#if !workspace}
    <p class="worktrees-message">Open a folder to see its Git worktrees.</p>
  {:else if loading && !result}
    <p class="worktrees-message">Loading worktrees…</p>
  {:else if error}
    <p class="worktrees-message">{error.includes('not inside a Git repository') ? 'This folder is not in a Git repository.' : error}</p>
  {:else if result}
    {#each result.worktrees as worktree (worktree.path)}
      <div class:current={worktree.current} class="worktree-row" title={worktree.path}>
        <span class="worktree-icon">⑂</span>
        <span class="worktree-details">
          <span class="worktree-branch">{worktree.branch ?? (worktree.detached ? 'Detached HEAD' : 'Bare repository')}</span>
          <span class="worktree-path">{worktree.path}</span>
        </span>
        {#if worktree.current}<span class="current-badge">CURRENT</span>{/if}
      </div>
    {:else}
      <p class="worktrees-message">No worktrees found.</p>
    {/each}
  {/if}
</section>

<style>
  .worktrees { flex:none; border-top:1px solid var(--line); padding-bottom:6px; }
  .worktrees-heading { height:32px; display:flex; align-items:center; justify-content:space-between; padding:0 10px 0 12px; color:var(--muted); font-size:calc(var(--ui-font-size,13px) * .77); font-weight:600; letter-spacing:.08em; }
  .refresh-button { border:0; background:transparent; color:var(--muted); border-radius:4px; cursor:pointer; font-size:16px; line-height:1; padding:3px 5px; }
  .refresh-button:hover { background:var(--hover); color:var(--text); }
  .refresh-button:disabled { opacity:.5; cursor:default; }
  .worktree-row { min-width:0; display:flex; align-items:center; gap:7px; min-height:34px; padding:5px 8px 5px 12px; color:var(--muted); }
  .worktree-row.current { color:var(--text); }
  .worktree-icon { flex:none; width:13px; color:var(--accent); font-size:15px; text-align:center; }
  .worktree-details { min-width:0; flex:1; display:flex; flex-direction:column; gap:2px; }
  .worktree-branch,.worktree-path { overflow:hidden; white-space:nowrap; text-overflow:ellipsis; }
  .worktree-branch { font-size:calc(var(--ui-font-size,13px) * .85); }
  .worktree-path { color:var(--dim); font-size:calc(var(--ui-font-size,13px) * .77); }
  .current-badge { flex:none; color:var(--accent); font-size:calc(var(--ui-font-size,13px) * .62); letter-spacing:.04em; }
  .worktrees-message { margin:0; padding:6px 12px 9px; color:var(--dim); font-size:calc(var(--ui-font-size,13px) * .77); line-height:1.5; overflow-wrap:anywhere; }
</style>
