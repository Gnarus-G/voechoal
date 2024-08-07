<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import type { AudioItem } from "./types";

  export let item: AudioItem;
  export let inert: boolean = false;

  $: is_playing = item.is_playing;

  async function play() {
    if (is_playing) {
      await invoke("player_pause", { id: item.id });
    } else {
      await invoke("player_start", { id: item.id });
    }
  }

  async function remove() {
    await invoke("delete_item", { id: item.id });
  }
</script>

<article
  class="relative flex justify-between items-center bg-slate-800 p-3 rounded-md hover:bg-slate-700 transition-colors"
>
  <button
    class="absolute -top-2 -left-2 bg-red-400 opacity-75 hover:opacity-100 hover:bg-red-500 transition-all active:scale-95 p-1 rounded-full text-white"
    on:click={remove}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      height="20px"
      viewBox="0 -960 960 960"
      width="20px"
      fill="currentColor"><path d="M200-440v-80h560v80H200Z" /></svg
    >
  </button>
  <div>
    {#if item.label}
      <h3 class="text-lg line-clamp-2">{item.label}</h3>
    {:else}
      <h3 class="text-lg">...</h3>
    {/if}

    <p class="text-slate-400 text-xs">{item.id}</p>
  </div>
  <button
    id="record_button"
    class="rounded-full p-2"
    disabled={inert}
    on:click={play}
  >
    {#if is_playing}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        height="32px"
        viewBox="0 -960 960 960"
        width="32px"
        fill="#e8eaed"
        ><path
          d="M360-320h80v-320h-80v320Zm160 0h80v-320h-80v320ZM480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0 227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z"
        /></svg
      >
    {:else}
      <svg
        xmlns="http://www.w3.org/2000/svg"
        height="32px"
        viewBox="0 -960 960 960"
        width="32px"
        fill="#e8eaed"
        ><path
          d="m380-300 280-180-280-180v360ZM480-80q-83 0-156-31.5T197-197q-54-54-85.5-127T80-480q0-83 31.5-156T197-763q54-54 127-85.5T480-880q83 0 156 31.5T763-763q54 54 85.5 127T880-480q0 83-31.5 156T763-197q-54 54-127 85.5T480-80Zm0-80q134 0 227-93t93-227q0-134-93-227t-227-93q-134 0-227 93t-93 227q0 134 93 227t227 93Zm0-320Z"
        /></svg
      >
    {/if}
  </button>
</article>

<style>
  button#record_button {
    background-color: theme("colors.blue.700");
  }

  button#record_button:hover {
    background-color: theme("colors.blue.800");
  }

  button#record_button:disabled {
    background-color: theme("colors.gray.600");
    color: theme("colors.gray.500");
  }
</style>
