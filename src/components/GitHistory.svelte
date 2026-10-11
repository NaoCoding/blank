<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';

  type Commit = { hash: string; parents: string[]; author: string; date: string; subject: string; refs: string[] };
  type GraphPath = { d: string; color: number };
  type GraphRow = { commit: Commit; lane: number; color: number; width: number; paths: GraphPath[] };
  let { workspace, onClose } = $props<{ workspace: string; onClose: () => void }>();
  let commits = $state<Commit[]>([]);
  let error = $state('');
  let loading = $state(true);

  function buildGraph(items: Commit[]): GraphRow[] {
    const lanes: string[] = [];
    const colors: number[] = [];
    let nextColor = 0;
    return items.map(commit => {
      let lane = lanes.indexOf(commit.hash);
      if (lane < 0) {
        lane = lanes.length;
        lanes.push(commit.hash);
        colors.push(nextColor++ % 6);
      }
      const commitColor = colors[lane];
      const before = [...lanes];
      const beforeColors = [...colors];
      const next = [...lanes];
      const nextColors = [...colors];
      next.splice(lane, 1);
      nextColors.splice(lane, 1);
      if (commit.parents[0] && !next.includes(commit.parents[0])) {
        next.splice(lane, 0, commit.parents[0]);
        nextColors.splice(lane, 0, commitColor);
      }
      let extraLane = lane + (commit.parents[0] ? 1 : 0);
      for (const parent of commit.parents.slice(1)) {
        if (!next.includes(parent)) {
          next.splice(extraLane, 0, parent);
          nextColors.splice(extraLane, 0, nextColor++ % 6);
        }
        extraLane++;
      }
      const paths: string[] = [];
      for (const [parentIndex, parent] of commit.parents.entries()) {
        const target = next.indexOf(parent);
        if (target >= 0) {
          const x1 = lane * 16 + 9;
          const x2 = target * 16 + 9;
          paths.push({ d: `M ${x1} 24 C ${x1} 34 ${x2} 38 ${x2} 48`, color: parentIndex === 0 ? commitColor : nextColors[target] });
        }
      }
      before.forEach((hash, oldLane) => {
        if (hash === commit.hash) return;
        const target = next.indexOf(hash);
        if (target < 0) return;
        const x1 = oldLane * 16 + 9;
        const x2 = target * 16 + 9;
        paths.push({ d: `M ${x1} 0 C ${x1} 18 ${x2} 30 ${x2} 48`, color: beforeColors[oldLane] });
      });
      lanes.splice(0, lanes.length, ...next);
      colors.splice(0, colors.length, ...nextColors);
      return { commit, lane, color: commitColor, width: Math.max(before.length, next.length, lane + 1), paths };
    });
  }

  let rows = $derived(buildGraph(commits));
  let graphWidth = $derived(Math.max(1, ...rows.map(row => row.width)) * 16 + 2);

  async function loadHistory() {
    loading = true;
    error = '';
    try { commits = await invoke<Commit[]>('git_history'); }
    catch (reason) { commits = []; error = String(reason); }
    finally { loading = false; }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') onClose();
  }

  $effect(() => { if (workspace) void loadHistory(); });
</script>

<svelte:window onkeydown={handleKeydown} />
<div class="history-backdrop" role="presentation" onclick={(event) => { if (event.target === event.currentTarget) onClose(); }}>
  <section class="history-dialog" role="dialog" aria-modal="true" aria-labelledby="git-history-title">
    <header class="history-header">
      <div><span class="history-eyebrow">GIT</span><h2 id="git-history-title">Commit history</h2><p title={workspace}>{workspace || 'No folder open'}</p></div>
      <div class="history-actions"><button class="history-refresh" onclick={loadHistory} disabled={loading} title="Refresh history" aria-label="Refresh history">↻</button><button class="history-close" onclick={onClose} aria-label="Close Git history">×</button></div>
    </header>
    {#if loading}
      <div class="history-message">Loading commit history…</div>
    {:else if error}
      <div class="history-message">{error.includes('not inside a Git repository') ? 'This folder is not in a Git repository.' : error}</div>
    {:else if !rows.length}
      <div class="history-message">No commits found.</div>
    {:else}
      <div class="history-list">
        {#each rows as row (row.commit.hash)}
          <article class="commit-row">
            <svg class="commit-graph" style={`width:${graphWidth}px`} viewBox={`0 0 ${graphWidth} 48`} aria-hidden="true">
              {#each row.paths as path}<path d={path.d} class={`graph-edge graph-color-${path.color}`} />{/each}
              <circle cx={row.lane * 16 + 9} cy="24" r={row.commit.parents.length > 1 ? 5 : 4} class:merge={row.commit.parents.length > 1} class={`graph-node graph-color-${row.color}`} />
            </svg>
            <div class="commit-info">
              <div class="commit-title"><strong title={row.commit.subject}>{row.commit.subject || '(no subject)'}</strong>{#each row.commit.refs as ref (ref)}<span class:remote={ref.includes('/')} class="branch-label">{ref}</span>{/each}{#if row.commit.parents.length > 1}<span class="merge-label">MERGE</span>{/if}</div>
              <div class="commit-meta"><code>{row.commit.hash.slice(0, 8)}</code><span>{row.commit.author}</span><span>{row.commit.date}</span></div>
            </div>
          </article>
        {/each}
      </div>
      <footer class="history-footer">Showing the latest {rows.length} commits across local and remote branches.</footer>
    {/if}
  </section>
</div>

<style>
  .history-backdrop { position:fixed; inset:0; z-index:12; display:grid; place-items:center; padding:5vh 5vw; background:#0009; }
  .history-dialog { width:min(1050px,92vw); height:min(820px,90vh); display:flex; flex-direction:column; overflow:hidden; color:var(--text); background:var(--panel); border:1px solid var(--line); border-radius:9px; box-shadow:0 20px 70px #0009; }
  .history-header { flex:none; display:flex; align-items:center; justify-content:space-between; padding:18px 22px 15px; border-bottom:1px solid var(--line); }
  .history-eyebrow { color:var(--accent); font-size:9px; font-weight:700; letter-spacing:.16em; }
  .history-header h2 { margin:3px 0 4px; font-size:18px; font-weight:600; }
  .history-header p { max-width:70vw; margin:0; overflow:hidden; color:var(--dim); font-size:11px; text-overflow:ellipsis; white-space:nowrap; }
  .history-actions { display:flex; align-items:center; gap:6px; }
  .history-refresh,.history-close { width:32px; height:32px; display:grid; place-items:center; padding:0; border:0; background:transparent; color:var(--muted); border-radius:5px; cursor:pointer; line-height:1; }
  .history-refresh { font-size:18px; }
  .history-close { font-size:22px; }
  .history-refresh:hover,.history-close:hover { background:var(--hover); color:var(--text); }
  .history-message { flex:1; display:grid; place-items:center; padding:24px; color:var(--muted); text-align:center; }
  .history-list { flex:1; overflow:auto; padding:8px 18px 10px; }
  .commit-row { min-height:48px; display:flex; align-items:stretch; border-bottom:1px solid color-mix(in srgb,var(--line) 55%,transparent); }
  .commit-graph { flex:none; height:48px; overflow:visible; }
  .graph-edge { fill:none; stroke-width:2; }
  .graph-node { stroke:var(--panel); stroke-width:2; }
  .graph-node.merge { fill:var(--panel); stroke-width:3; }
  .graph-color-0 { stroke:#69b3a2; fill:#69b3a2; }
  .graph-color-1 { stroke:#d49a55; fill:#d49a55; }
  .graph-color-2 { stroke:#879be0; fill:#879be0; }
  .graph-color-3 { stroke:#d17aa7; fill:#d17aa7; }
  .graph-color-4 { stroke:#84b65f; fill:#84b65f; }
  .graph-color-5 { stroke:#b18ada; fill:#b18ada; }
  .commit-info { min-width:0; flex:1; align-self:center; padding:6px 10px; }
  .commit-title { min-width:0; display:flex; align-items:center; gap:7px; }
  .commit-title strong { min-width:0; overflow:hidden; font-size:12px; font-weight:500; text-overflow:ellipsis; white-space:nowrap; }
  .branch-label,.merge-label { flex:none; max-width:180px; overflow:hidden; padding:2px 6px; border:1px solid color-mix(in srgb,var(--accent) 45%,transparent); border-radius:10px; color:var(--accent); font-size:9px; text-overflow:ellipsis; white-space:nowrap; }
  .branch-label.remote { border-color:var(--line); color:var(--dim); }
  .merge-label { border-color:#879be055; color:#9cace5; }
  .commit-meta { display:flex; align-items:center; gap:12px; margin-top:4px; color:var(--dim); font-size:10px; }
  .commit-meta code { color:var(--muted); font:10px var(--mono); }
  .history-footer { flex:none; padding:8px 20px; border-top:1px solid var(--line); color:var(--dim); font-size:10px; }
  @media(max-width:650px) { .history-backdrop{padding:2vh 2vw}.history-dialog{width:96vw;height:96vh}.history-header{padding:14px}.commit-meta{gap:7px}.commit-meta span:nth-child(2){display:none}.branch-label{max-width:100px} }
</style>
