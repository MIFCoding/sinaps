<script lang="ts">
import { initials } from './api';
let { name, size = 32 }: {
    name: string;
    size?: number;
} = $props();
let image = $derived(name === 'Анна Петрова'
    ? '/design/anna.png'
    : name === 'Иван Сергеевич'
        ? '/design/ivan.png'
        : null);
</script>

<span class="person-avatar" style:width={`${size}px`} style:height={`${size}px`}>
  {#if image}<img src={image} alt="" />{:else}{initials(name)}{/if}
</span>
