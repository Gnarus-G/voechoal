<script lang="ts">
  import type { AudioItem } from "./types";

  export let item: AudioItem;

  let is_playing = false;
  async function play() {
    is_playing = !is_playing;
  }
</script>

<article class="flex justify-between items-center bg-slate-800 p-3 rounded-md">
  <div>
    <h3 class="text-lg">{item.excerpt}</h3>
    <p class="text-slate-400 text-xs">{item.id}</p>
  </div>
  <button
    class="bg-blue-700 hover:bg-blue-800 rounded-full p-2"
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
