<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen, type UnlistenFn } from '@tauri-apps/api/event';
  import { Terminal } from '@xterm/xterm';
  import '@xterm/xterm/css/xterm.css';
  let { cwd, dark } = $props<{ cwd: string; dark: boolean }>();
  let host: HTMLDivElement; let terminal: Terminal; let id = ''; let cleanup: UnlistenFn[] = [];
  onMount(async () => {
    const fontFamily = getComputedStyle(host).getPropertyValue('--mono').trim();
    terminal = new Terminal({ cursorBlink: true, fontFamily, fontSize: 13, theme: dark ? { background: '#111315', foreground: '#d9dadd', cursor: '#d6a76c' } : { background: '#fff', foreground: '#25282c', cursor: '#98682d' }, convertEol: true });
    terminal.open(host);
    const fit = () => {
      const fontSize = terminal.options.fontSize ?? 12;
      const cols = Math.max(2, Math.floor((host.clientWidth - 24) / (fontSize * 0.6)));
      const rows = Math.max(2, Math.floor((host.clientHeight - 12) / (fontSize * 1.2)));
      if (terminal.cols !== cols || terminal.rows !== rows) terminal.resize(cols, rows);
      if (id) void invoke('terminal_resize', { id, cols, rows }).catch(() => {});
    };
    fit();
    try {
      id = await invoke<string>('start_terminal', { cwd, cols: terminal.cols, rows: terminal.rows });
      cleanup.push(await listen<number[]>(`terminal-output-${id}`, e => terminal.write(new Uint8Array(e.payload))));
      cleanup.push(await listen(`terminal-exit-${id}`, () => terminal.writeln('\r\n[process exited]')));
      terminal.onData(data => invoke('terminal_write', { id, data }).catch(console.error));
      const resize = new ResizeObserver(fit); resize.observe(host);
      cleanup.push(() => resize.disconnect());
    } catch (e) { terminal.writeln(`Unable to start terminal: ${String(e)}`); }
  });
  onDestroy(() => { cleanup.forEach(fn => fn()); terminal?.dispose(); if (id) invoke('close_terminal', { id }).catch(() => {}); });
</script>
<div class="terminal-host" bind:this={host}></div>
