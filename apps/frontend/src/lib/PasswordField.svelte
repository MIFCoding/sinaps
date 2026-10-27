<script lang="ts">
  import { Eye, EyeOff } from '@lucide/svelte';
  let {
    id,
    value = $bindable(''),
    autocomplete = 'current-password',
    minlength,
    maxlength,
    placeholder,
    required = true,
  }: {
    id: string;
    value?: string;
    autocomplete?: 'current-password' | 'new-password';
    minlength?: number;
    maxlength?: number;
    placeholder?: string;
    required?: boolean;
  } = $props();
  let visible = $state(false);
</script>

<div class="password-field">
  <input
    {id}
    type={visible ? 'text' : 'password'}
    bind:value
    {autocomplete}
    {minlength}
    {maxlength}
    {placeholder}
    {required}
  />
  <button
    type="button"
    class="password-toggle"
    aria-label={visible ? 'Скрыть пароль' : 'Показать пароль'}
    aria-controls={id}
    aria-pressed={visible}
    title={visible ? 'Скрыть пароль' : 'Показать пароль'}
    onclick={() => (visible = !visible)}
  >
    {#if visible}<EyeOff size={19} />{:else}<Eye size={19} />{/if}
  </button>
</div>

<style>
  .password-field {
    position: relative;
  }
  input {
    padding-right: 52px;
  }
  .password-toggle {
    position: absolute;
    right: 3px;
    top: 50%;
    transform: translateY(-50%);
    width: 42px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 0;
    border-radius: 8px;
    color: #718c9a;
    background: transparent;
  }
  .password-toggle:hover {
    color: #219f90;
    background: #e4f5f1;
  }
</style>
