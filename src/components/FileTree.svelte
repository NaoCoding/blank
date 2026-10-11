<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { displayFilePath } from '../lib/paths';
  import FileTree from './FileTree.svelte';
  type Entry = { name: string; path: string; is_dir: boolean };
  let { path, depth = 0, refreshKey = 0, onOpen } = $props<{ path: string; depth?: number; refreshKey?: number; onOpen: (path: string) => void }>();
  let entries: Entry[] = $state([]); let expanded = $state(false); let loading = $state(false); let loadRequest = 0;
  async function loadDirectory(directory: string) {
    const request = ++loadRequest;
    loading = true;
    try {
      const result = await invoke<Entry[]>('list_directory', { path: directory });
      if (request === loadRequest) entries = result;
    } catch (e) {
      console.error(e);
    } finally {
      if (request === loadRequest) loading = false;
    }
  }
  $effect(() => {
    const directory = path;
    const refresh = refreshKey;
    if (depth === 0) {
      expanded = true;
      void loadDirectory(directory);
    } else if (expanded) {
      void loadDirectory(directory);
    } else {
      expanded = false;
      entries = [];
    }
    void refresh;
  });
  function toggle() { expanded = !expanded; }
</script>
<div class="tree-node">
  <button class="tree-row folder-row" style={`--depth:${depth}`} onclick={toggle} title={displayFilePath(path)}>
    <span class="tree-spacer"></span><span class="file-icon folder-icon">{expanded ? '▾' : '▸'}</span><span class="tree-label">{path.split(/[\\/]/).pop()}</span>{#if loading}<span class="muted">…</span>{/if}
  </button>
  {#if expanded}{#each entries as item (item.path)}
    {#if item.is_dir}<FileTree path={item.path} depth={depth + 1} {refreshKey} {onOpen} />{:else}<button class="tree-row" style={`--depth:${depth + 1}`} onclick={() => onOpen(item.path)} title={displayFilePath(item.path)}><span class="tree-spacer"></span><span class="file-icon">{item.name.endsWith('.md') ? '◈' : '·'}</span><span class="tree-label">{item.name}</span></button>{/if}
  {/each}{/if}
</div>
