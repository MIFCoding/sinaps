<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { isTauri } from '@tauri-apps/api/core';
  import Brand from './Brand.svelte';
  import PasswordField from './PasswordField.svelte';
  import { api, type User } from './api';
  let { onauthenticated }: { onauthenticated: (user: User) => Promise<void> } = $props();
  const demo = new URLSearchParams(location.search).get('demo') === '1' || isTauri();
  const authFragment = new URLSearchParams(location.hash.slice(1));
  const invitation = authFragment.get('invite') ?? '';
  const verificationToken = authFragment.get('verify') ?? '';
  const resetToken = authFragment.get('reset') ?? '';
  type DemoProfile = 'doctor' | 'patient';
  const demoCredentials: Record<DemoProfile, { email: string; password: string }> = {
    doctor: { email: 'doctor@synapse.demo', password: 'Synapse2026!' },
    patient: { email: 'patient@synapse.demo', password: 'Synapse2026!' },
  };
  let mode = $state<'login' | 'register' | 'invitation' | 'recovery' | 'reset' | 'verification'>(
    resetToken ? 'reset' : invitation ? 'invitation' : 'login',
  );
  let mounted = true;
  onDestroy(() => {
    mounted = false;
  });
  let email = $state(demo ? 'doctor@synapse.demo' : '');
  let password = $state(demo ? 'Synapse2026!' : '');
  let demoProfile = $state<DemoProfile>('doctor');
  let firstName = $state(''),
    lastName = $state(''),
    confirmation = $state('');
  let consent = $state(false),
    busy = $state(false),
    error = $state(''),
    notice = $state('');
  let emailActionsAvailable = $state(false);
  let existingAccount = $state(false),
    invitationReady = $state(!invitation),
    invitationDoctor = $state('');
  onMount(() => {
    let alive = true;
    api('auth_config')
      .then((value) => {
        if (alive) emailActionsAvailable = value.email_actions_available;
      })
      .catch(() => {});
    if (verificationToken) {
      api('verify_email', { body: { token: verificationToken } })
        .then(async (user) => {
          if (!alive) return;
          history.replaceState(null, '', location.pathname + location.search);
          await onauthenticated(user);
        })
        .catch(() => {
          if (alive) error = 'Ссылка подтверждения недействительна или срок её действия истёк.';
        });
      return () => {
        alive = false;
      };
    }
    if (!invitation)
      return () => {
        alive = false;
      };
    api('invitation_preview', { params: { code: invitation } })
      .then((value) => {
        if (alive) {
          existingAccount = value.existing_account;
          invitationDoctor = value.doctor_name;
          invitationReady = true;
        }
      })
      .catch(() => {
        if (alive)
          error =
            'Приглашение недействительно или срок его действия истёк. Запросите новую ссылку у врача.';
      });
    return () => {
      alive = false;
    };
  });
  function switchMode(next: 'login' | 'register') {
    mode = next;
    password = '';
    confirmation = '';
    error = '';
    notice = '';
  }
  function backToLogin() {
    mode = 'login';
    error = '';
    notice = '';
    history.replaceState(null, '', location.pathname + location.search);
  }
  function selectDemoProfile(profile: DemoProfile) {
    demoProfile = profile;
    email = demoCredentials[profile].email;
    password = demoCredentials[profile].password;
    error = '';
  }
  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    error = '';
    notice = '';
    if (
      (mode === 'register' || mode === 'reset' || (mode === 'invitation' && !existingAccount)) &&
      password !== confirmation
    ) {
      error = 'Пароли не совпадают';
      return;
    }
    if ((mode === 'register' || (mode === 'invitation' && !existingAccount)) && !consent) {
      error = 'Подтвердите согласие на обработку персональных данных';
      return;
    }
    busy = true;
    try {
      if (mode === 'recovery') {
        await api('request_password_reset', { body: { email } });
        notice = 'Если для этого адреса есть аккаунт, мы отправили ссылку для сброса пароля.';
        return;
      }
      if (mode === 'verification') {
        await api('request_email_verification', { body: { email } });
        notice = 'Если аккаунт ожидает подтверждения, мы отправили новое письмо.';
        return;
      }
      if (mode === 'reset') {
        await api('complete_password_reset', {
          body: { token: resetToken, password, password_confirmation: confirmation },
        });
        history.replaceState(null, '', location.pathname + location.search);
        mode = 'login';
        password = '';
        confirmation = '';
        notice = 'Пароль изменён. Теперь войдите с новым паролем.';
        return;
      }
      let user: User;
      if (mode === 'register') {
        const result = await api('register', {
          body: {
            email,
            first_name: firstName,
            last_name: lastName,
            password,
            password_confirmation: confirmation,
            consent,
          },
        });
        if ('verification_required' in result && result.verification_required) {
          email = result.email;
          mode = 'verification';
          notice = 'Мы отправили письмо. Перейдите по ссылке в нём, чтобы подтвердить почту.';
          return;
        }
        user = result as User;
      } else if (mode === 'invitation') {
        const result = await api('activate', { body: { code: invitation, password } });
        if ('verification_required' in result && result.verification_required) {
          email = result.email;
          mode = 'verification';
          notice = 'Связь с врачом подтверждена. Завершите подключение по ссылке в письме.';
          return;
        }
        user = result as User;
      } else {
        user = await api('login', { body: { email, password } });
      }
      if (!mounted) return;
      password = '';
      confirmation = '';
      if (invitation) history.replaceState(null, '', location.pathname + location.search);
      await onauthenticated(user);
    } catch (cause) {
      error = cause instanceof Error ? cause.message : 'Не удалось войти';
    } finally {
      busy = false;
    }
  }
</script>

<main class="auth-page" class:auth-registration={mode === 'register' || mode === 'invitation'}>
  {#if mode === 'register' || mode === 'invitation'}<header class="auth-header">
      <Brand />
    </header>{/if}
  <div class="auth-layout">
    <section class="auth-story">
      <Brand size={mode === 'register' || mode === 'invitation' ? 70 : 88} caption />
    </section>
    <section class="auth-form" aria-labelledby="auth-title">
      <h1 id="auth-title">
        {mode === 'login'
          ? 'Добро пожаловать!'
          : mode === 'register'
            ? 'Регистрация'
            : mode === 'invitation'
              ? 'Приглашение врача'
              : mode === 'recovery'
                ? 'Восстановление пароля'
                : mode === 'reset'
                  ? 'Новый пароль'
                  : 'Подтверждение почты'}
      </h1>
      {#if notice}<p class="feedback success" role="status">{notice}</p>{/if}
      {#if mode === 'invitation'}<p class="muted">
          Приглашение от врача: {invitationDoctor || 'Загружаем'}.
          {existingAccount
            ? 'Введите пароль вашего аккаунта, чтобы принять приглашение.'
            : 'Создайте пароль, чтобы принять приглашение и открыть свою карточку.'}
        </p>{/if}
      <form onsubmit={submit}>
        {#if mode === 'login' || mode === 'register' || (mode === 'recovery' && emailActionsAvailable)}
          <label class="visually-hidden" for="auth-email"
            >{mode === 'recovery' ? 'Электронная почта' : 'Номер телефона или почта'}</label
          >
          <input
            id="auth-email"
            type={mode === 'recovery' ? 'email' : 'text'}
            placeholder={mode === 'recovery' ? 'Электронная почта' : 'Номер телефона или почта'}
            bind:value={email}
            autocomplete="username"
            maxlength="254"
            required
          />
        {/if}
        {#if mode === 'register'}
          <label class="visually-hidden" for="auth-first">Имя</label>
          <input
            id="auth-first"
            placeholder="Имя"
            bind:value={firstName}
            autocomplete="given-name"
            maxlength="60"
            required
          />
          <label class="visually-hidden" for="auth-last">Фамилия</label>
          <input
            id="auth-last"
            placeholder="Фамилия"
            bind:value={lastName}
            autocomplete="family-name"
            maxlength="59"
            required
          />
        {/if}
        {#if mode === 'login' || mode === 'register' || mode === 'invitation' || mode === 'reset'}
          <label class="visually-hidden" for="auth-password"
            >{mode === 'reset' ? 'Новый пароль' : 'Пароль'}</label
          >
          <PasswordField
            id="auth-password"
            bind:value={password}
            placeholder={mode === 'reset' ? 'Новый пароль' : 'Пароль'}
            autocomplete={mode === 'login' || (mode === 'invitation' && existingAccount)
              ? 'current-password'
              : 'new-password'}
            minlength={mode === 'login' || (mode === 'invitation' && existingAccount)
              ? undefined
              : 8}
            maxlength={128}
          />
        {/if}
        {#if mode === 'register' || mode === 'reset' || (mode === 'invitation' && !existingAccount)}
          <label class="visually-hidden" for="auth-confirmation">Повторите пароль</label>
          <PasswordField
            id="auth-confirmation"
            bind:value={confirmation}
            placeholder="Повторите пароль"
            autocomplete="new-password"
            minlength={8}
            maxlength={128}
          />
        {/if}
        {#if mode === 'register' || (mode === 'invitation' && !existingAccount)}
          <label class="auth-consent"
            ><input type="checkbox" bind:checked={consent} required />Согласен на обработку
            персональных данных</label
          >
        {/if}
        {#if mode === 'login'}
          <button
            class="auth-forgot"
            type="button"
            onclick={() => {
              mode = 'recovery';
              error = '';
              notice = '';
            }}>Забыли пароль?</button
          >
        {/if}
        {#if mode === 'recovery' && !emailActionsAvailable}<p class="auth-help" role="status">
            Восстановление пароля пока выполняет администратор стенда. Обратитесь к организатору
            демонстрации.
          </p>{/if}
        {#if error}<div class="feedback error" role="alert">{error}</div>{/if}
        <button
          class="primary wide"
          type="submit"
          disabled={busy ||
            (mode === 'invitation' && !invitationReady) ||
            (mode === 'recovery' && !emailActionsAvailable)}
          >{busy
            ? 'Подождите'
            : mode === 'login'
              ? 'Войти'
              : mode === 'register'
                ? 'Создать аккаунт'
                : mode === 'invitation'
                  ? 'Принять приглашение'
                  : mode === 'recovery'
                    ? 'Отправить ссылку'
                    : mode === 'reset'
                      ? 'Сохранить новый пароль'
                      : 'Отправить письмо ещё раз'}</button
        >
      </form>
      {#if mode === 'login' || mode === 'register'}<div class="auth-switch-row">
          <span>{mode === 'login' ? 'Нет аккаунта?' : 'Уже есть аккаунт?'}</span>
          <button
            disabled={busy}
            onclick={() => switchMode(mode === 'login' ? 'register' : 'login')}
            >{mode === 'login' ? 'Зарегистрироваться' : 'Войти'}</button
          >
        </div>
      {:else if mode === 'recovery' || mode === 'reset' || mode === 'verification'}<div
          class="auth-switch-row"
        >
          <button
            disabled={busy}
            onclick={backToLogin}>Вернуться ко входу</button
          >
        </div>{/if}
      {#if demo && mode === 'login'}<aside class="demo-box">
          <span class="eyebrow">ДЕМОНСТРАЦИОННЫЙ СТЕНД</span>
          <p>Выберите профиль для входа</p>
          <div class="demo-buttons" role="group" aria-label="Демо-профиль">
            <button
              class:chosen={demoProfile === 'doctor'}
              aria-pressed={demoProfile === 'doctor'}
              onclick={() => {
                selectDemoProfile('doctor');
              }}>Врач</button
            >
            <button
              class:chosen={demoProfile === 'patient'}
              aria-pressed={demoProfile === 'patient'}
              onclick={() => {
                selectDemoProfile('patient');
              }}>Пациент</button
            >
          </div>
        </aside>{/if}
    </section>
  </div>
</main>
