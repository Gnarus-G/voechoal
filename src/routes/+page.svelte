<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";
  import type { PollingState } from "$lib/types";
  import Audio from "$lib/Audio.svelte";

  let is_recording = false;

  async function toggle() {
    if (is_recording) {
      await invoke("record_pause");
      is_recording = false;
    } else {
      await invoke("record_start");
      is_recording = true;
    }
  }

  let state: PollingState = {
    is_transcribing: false,
    audio_items: [],
  };

  onMount(() => {
    setInterval(async () => {
      state = await invoke("poll_recordings");
      /* console.log("items", state.audio_items); */
    }, 250);
  });
</script>

<div class="container mx-auto">
  <ul class="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-5 py-5">
    {#each state.audio_items as item}
      <Audio {item} />
    {/each}
  </ul>

  <section class="fixed bottom-0 w-full flex justify-center py-3">
    <button
      class="toggle relative"
      on:click={toggle}
      data-recording={is_recording}
    >
      <span
        class="{is_recording
          ? 'animate-ping'
          : ''} absolute top-0 left-0 inline-flex h-full w-full rounded-full bg-fuchsia-600"
      ></span>
      {#if is_recording}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          height="24px"
          viewBox="0 -960 960 960"
          width="24px"
          fill="#e8eaed"
          ><path
            d="M480-400q-50 0-85-35t-35-85v-240q0-50 35-85t85-35q50 0 85 35t35 85v240q0 50-35 85t-85 35Zm0-240Zm-40 520v-123q-104-14-172-93t-68-184h80q0 83 58.5 141.5T480-320q83 0 141.5-58.5T680-520h80q0 105-68 184t-172 93v123h-80Zm40-360q17 0 28.5-11.5T520-520v-240q0-17-11.5-28.5T480-800q-17 0-28.5 11.5T440-760v240q0 17 11.5 28.5T480-480Z"
          /></svg
        >
      {:else}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          height="24px"
          viewBox="0 -960 960 960"
          width="24px"
          fill="#e8eaed"
          ><path
            d="m710-362-58-58q14-23 21-48t7-52h80q0 44-13 83.5T710-362ZM480-594Zm112 112-72-72v-206q0-17-11.5-28.5T480-800q-17 0-28.5 11.5T440-760v126l-80-80v-46q0-50 35-85t85-35q50 0 85 35t35 85v240q0 11-2.5 20t-5.5 18ZM440-120v-123q-104-14-172-93t-68-184h80q0 83 57.5 141.5T480-320q34 0 64.5-10.5T600-360l57 57q-29 23-63.5 39T520-243v123h-80Zm352 64L56-792l56-56 736 736-56 56Z"
          /></svg
        >
      {/if}
    </button>
  </section>
</div>

<style>
  button.toggle {
    padding: theme("spacing.5");
    border-radius: theme("borderRadius.full");
    background: theme("colors.fuchsia.900");
    transition: transform 125ms ease-in-out;
  }

  button.toggle > span {
    opacity: 0;
  }

  button.toggle[data-recording="true"] > span {
    opacity: theme("opacity.75");
  }

  button.toggle:hover {
    background: theme("colors.fuchsia.800");
    transform: scale(1.2);
  }
</style>
