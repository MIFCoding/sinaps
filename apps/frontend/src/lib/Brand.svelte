<script lang="ts">
let { size = 32, caption = false }: {
    size?: number;
    caption?: boolean;
} = $props();
</script>

<div class="brand" class:brand-caption={caption}>
  <span class="brand-symbol" style:width={`${size}px`} style:height={`${size}px`}>
    <img
      src="/design/synapse-logo.svg?v=20261025-no-text"
      alt=""
      style:transform={`scale(${size / 32})`}
    />
  </span>
  <span class="brand-name"
    >СИНАПС{#if caption}<small>Точка связи. Пространство заботы.</small>{/if}</span
  >
</div>
