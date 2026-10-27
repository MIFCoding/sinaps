<script lang="ts">
  import { onMount } from 'svelte';
  import type { User } from './api';

  let { user }: { user: User } = $props();
  let now = $state(new Date());
  let greeting = $derived(
    now.getHours() >= 5 && now.getHours() < 12
      ? 'Доброе утро'
      : now.getHours() < 18
        ? 'Добрый день'
        : 'Добрый вечер',
  );
  let doctorDate = $derived.by(() => {
    const formatted = new Intl.DateTimeFormat('ru-RU', {
      day: 'numeric',
      month: 'long',
      weekday: 'long',
    }).format(now);
    return `${formatted.charAt(0).toLocaleUpperCase('ru-RU')}${formatted.slice(1)}`;
  });
  onMount(() => {
    const update = () => {
      now = new Date();
    };
    const timer = window.setInterval(update, 60000);
    window.addEventListener('focus', update);
    document.addEventListener('visibilitychange', update);
    return () => {
      clearInterval(timer);
      window.removeEventListener('focus', update);
      document.removeEventListener('visibilitychange', update);
    };
  });
</script>

<div class="home-greeting">
  <h2>{greeting}, {user.role === 'doctor' ? user.name : user.name.split(' ')[0]}</h2>
  <p class="muted">
    {user.role === 'doctor' ? doctorDate : 'Как вы себя чувствуете сегодня?'}
  </p>
</div>
