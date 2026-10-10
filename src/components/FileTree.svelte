<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import FileTree from './FileTree.svelte';
  type Entry = { name: string; path: string; is_dir: boolean };
  let { path, depth = 0, onOpen } = $props<{ path: string; depth?: number; onOpen: (path: string) => void }>();
  let entries: Entry[] = $state([]); let expanded = $state(false); let loading = $state(false);
  async function toggle() {
    if (!expanded && !entries.length) { loading = true; try { entries = await invoke<Entry[]>('list_directory', { path }); } catch (e) { console.error(e); } finally { loading = false; } }
    expanded = !expanded;
  }
</script>
<div class="tree-node">
  <button class="tree-row folder-row" style={`--depth:${depth}`} onclick={toggle} title={path}>
    <span class="chevron">{expanded ? '⌄' : '›'}</span><span class="file-icon folder-icon">{expanded ? '▾' : '▸'}</span><span class="tree-label">{path.split(/[\\/]/).pop()}</span>{#if loading}<span class="muted">…</span>{/if}
  </button>
  {#if expanded}{#each entries as item (item.path)}
    {#if item.is_dir}<FileTree path={item.path} depth={depth + 1} {onOpen} />{:else}<button class="tree-row" style={`--depth:${depth + 1}`} onclick={() => onOpen(item.path)} title={item.path}><span class="tree-spacer"></span><span class="file-icon">{item.name.endsWith('.md') ? '◈' : '·'}</span><span class="tree-label">{item.name}</span></button>{/if}
  {/each}{/if}
</div>
