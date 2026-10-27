<script lang="ts">
  import { onMount } from 'svelte';
  import { Check, Copy, KeyRound, RefreshCw, X } from '@lucide/svelte';
  import { api, ApiError, date, type Patient, type Invitation } from './api';
  import { historyFields, comorbidityLabels } from './medicalHistory';
  let {
    mode,
    patient,
    onclose,
    onsaved,
    onerror,
  }: {
    mode: 'create' | 'edit' | 'invite';
    patient: Patient | null;
    onclose: () => void;
    onsaved: (patient: Patient) => Promise<void>;
    onerror: (cause: unknown) => void;
  } = $props();
  let stage = $state<'create' | 'edit' | 'invite'>('create');
  let profile = $state<Patient | null>(null);
  let name = $state(''),
    email = $state(''),
    birth = $state(''),
    started = $state('');
  let summary = $state(''),
    allergies = $state(''),
    status = $state<Patient['status']>('active');
  let busy = $state(false),
    error = $state(''),
    stale = $state(false),
    copied = $state(false);
  let invitation = $state<Invitation | null>(null);
  let height = $state<number | undefined>(),
    weight = $state<number | undefined>(),
    bloodGroup = $state('');
  let medicalHistory = $state<Patient['medical_history']>({
    complaints: '',
    present_illness: '',
    life_history: '',
    comorbidities_status: 'unknown',
    comorbidities_details: '',
  });
  const requestId = crypto.randomUUID();
  let invitationLink = $derived(
    invitation
      ? `${import.meta.env.VITE_PUBLIC_APP_URL || (location.hostname === 'tauri.localhost' || location.protocol === 'tauri:' ? 'http://10.0.2.2:5173' : location.origin)}/#invite=${invitation.code}`
      : '',
  );
  const today = new Date().toISOString().slice(0, 10);
  function assign(value: Patient) {
    profile = value;
    name = value.name;
    email = value.email;
    birth = value.birth_date;
    started = value.observation_started;
    status = value.status;
    summary = value.medical_summary;
    allergies = value.allergies ?? '';
    medicalHistory = { ...value.medical_history };
    height = value.physical_info.height_cm ?? undefined;
    weight = value.physical_info.weight_kg ?? undefined;
    bloodGroup = value.physical_info.blood_group ?? '';
    stale = false;
  }
  onMount(() => {
    stage = mode;
    if (patient) assign(patient);
  });
  function dialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    const cancel = (event: Event) => {
      event.preventDefault();
      if (!busy) onclose();
    };
    node.addEventListener('cancel', cancel);
    return {
      destroy() {
        node.removeEventListener('cancel', cancel);
        node.close();
        previous?.focus();
      },
    };
  }
  function fail(cause: unknown) {
    if (cause instanceof ApiError && cause.status === 401) {
      onerror(cause);
      return;
    }
    error = cause instanceof Error ? cause.message : 'Не удалось сохранить данные';
    stale = cause instanceof ApiError && cause.code === 'stale_version';
  }
  async function issue() {
    if (!profile) return;
    busy = true;
    error = '';
    copied = false;
    try {
      invitation = await api('create_invitation', {
        params: { id: profile.id },
      });
    } catch (cause) {
      fail(cause);
    } finally {
      busy = false;
    }
  }
  async function save(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    busy = true;
    error = '';
    try {
      if (stage === 'create') {
        const created = await api('create_patient', {
          body: {
            id: requestId,
            name,
            email,
            birth_date: birth,
          },
        });
        assign(created);
        stage = 'invite';
        await onsaved(created);
        await issue();
      } else if (profile) {
        const saved = await api('update_patient', {
          params: { id: profile.id },
          body: {
            version: profile.version,
            name,
            birth_date: birth,
            observation_started: started,
            status,
            medical_summary: summary,
            allergies: allergies.trim() || null,
            physical_info: {
              height_cm: height ?? null,
              weight_kg: weight ?? null,
              blood_group: bloodGroup || null,
            },
            medical_history: {
              ...medicalHistory,
              comorbidities_details:
                medicalHistory.comorbidities_status === 'present'
                  ? medicalHistory.comorbidities_details
                  : '',
            },
          },
        });
        await onsaved(saved);
        onclose();
      }
    } catch (cause) {
      fail(cause);
    } finally {
      busy = false;
    }
  }
  async function reload() {
    if (!profile) return;
    busy = true;
    try {
      assign(
        await api('get_patient', {
          params: { id: profile.id },
        }),
      );
      error = '';
    } catch (cause) {
      fail(cause);
    } finally {
      busy = false;
    }
  }
  async function copy() {
    if (!invitation) return;
    try {
      await navigator.clipboard.writeText(invitationLink);
      copied = true;
    } catch {
      error = 'Автоматическое копирование недоступно. Выделите и скопируйте код вручную.';
    }
  }
</script>

<dialog class="modal card" use:dialog aria-labelledby="patient-manager-title">
  <div class="section-heading">
    <div>
      <span class="eyebrow">{stage === 'create' ? 'НОВОЕ НАБЛЮДЕНИЕ' : profile?.name}</span>
      <h2 id="patient-manager-title">
        {stage === 'create'
          ? 'Новый пациент'
          : stage === 'edit'
            ? 'Редактировать карточку'
            : 'Подключение пациента'}
      </h2>
    </div>
    <button class="icon-button" aria-label="Закрыть" disabled={busy} onclick={onclose}
      ><X size={21} /></button
    >
  </div>
  {#if stage === 'invite'}
    <div class="invitation-intro">
      <KeyRound size={26} />
      <p>
        Передайте пациенту ссылку-приглашение. Он откроет её и задаст собственный пароль. Вводить
        код доступа не нужно.
      </p>
    </div>
    <div class="invitation-account">
      <span class="muted">Логин пациента</span><strong>{profile?.email}</strong>
    </div>
    {#if invitation}
      <label for="invitation-code">Ссылка-приглашение</label>
      <textarea
        id="invitation-code"
        aria-label="Ссылка-приглашение"
        rows="3"
        readonly
        value={invitationLink}></textarea>
      <p class="muted small-text">
        Действует до {date(invitation.expires_at, true)}. Ссылка открывает доступ к карточке,
        передавайте его только этому пациенту.
      </p>
      <button class="secondary wide" onclick={copy}
        ><Copy size={16} />{copied ? 'Ссылка скопирована' : 'Скопировать ссылку'}</button
      >
    {:else}<p class="muted form-intro">
        Создайте приглашение. Оно действует 72 часа и может быть принято один раз.
      </p>{/if}
    {#if error}<div class="feedback error" role="alert">{error}</div>{/if}
    <div class="modal-actions">
      <button class="secondary" disabled={busy} onclick={issue}
        ><RefreshCw size={16} />{invitation ? 'Новое приглашение' : 'Создать приглашение'}</button
      ><button class="primary" disabled={busy} onclick={onclose}>Готово<Check size={16} /></button>
    </div>
    {#if invitation}<p class="muted small-text">
        Новое приглашение отменит предыдущую ссылку. После закрытия окна ссылку нельзя просмотреть
        повторно. Создайте новое приглашение.
      </p>{/if}
  {:else}
    <form onsubmit={save}>
      <label for="patient-name">ФИО пациента</label><input
        id="patient-name"
        bind:value={name}
        maxlength="120"
        autocomplete="off"
        required
      />
      {#if stage === 'create'}
        <label for="patient-email">Телефон или почта</label><input
          id="patient-email"
          type="text"
          bind:value={email}
          maxlength="254"
          autocomplete="off"
          required
        />
        <p class="muted small-text">
          Телефон или почта используются как логин. Подтверждение контакта не требуется.
        </p>
      {/if}
      <label for="patient-birth">Дата рождения</label><input
        id="patient-birth"
        type="date"
        min="1900-01-01"
        max={today}
        bind:value={birth}
        required
      />
      {#if stage === 'edit'}
        <label for="patient-started">Начало наблюдения</label><input
          id="patient-started"
          type="date"
          min={birth}
          max={today}
          bind:value={started}
          required
        />
        <label for="patient-status">Статус наблюдения</label><select
          id="patient-status"
          bind:value={status}
          ><option value="active">Активный</option><option value="observation">На наблюдении</option
          ><option value="archived">В архиве</option></select
        >
        <fieldset class="medical-history-editor">
          <legend>Основная информация</legend>
          <label for="patient-height">Рост, см</label><input
            id="patient-height"
            type="number"
            min="0.1"
            max="400"
            step="0.1"
            bind:value={height}
            placeholder="Нет сведений"
          />
          <label for="patient-weight">Вес, кг</label><input
            id="patient-weight"
            type="number"
            min="0.1"
            max="1000"
            step="0.1"
            bind:value={weight}
            placeholder="Нет сведений"
          />
          <label for="patient-blood">Группа крови</label><select
            id="patient-blood"
            bind:value={bloodGroup}
          >
            <option value="">Нет сведений</option>
            {#each ['O (I) Rh+', 'O (I) Rh−', 'A (II) Rh+', 'A (II) Rh−', 'B (III) Rh+', 'B (III) Rh−', 'AB (IV) Rh+', 'AB (IV) Rh−'] as value}<option
                {value}>{value}</option
              >{/each}
          </select>
        </fieldset>
        <label for="patient-summary">Медицинские сведения</label><textarea
          id="patient-summary"
          rows="5"
          bind:value={summary}
          maxlength="6000"></textarea>
        <label for="patient-allergies">Аллергии</label><textarea
          id="patient-allergies"
          rows="2"
          bind:value={allergies}
          maxlength="2000"
          placeholder="Нет сведений"></textarea>
        <fieldset class="medical-history-editor">
          <legend>Анамнез и сопутствующие состояния</legend>
          {#each historyFields as [key, label]}
            <label for={`history-${key}`}>{label}</label>
            <textarea
              id={`history-${key}`}
              rows="3"
              maxlength="6000"
              bind:value={medicalHistory[key]}
              placeholder="Нет сведений"></textarea>
          {/each}
          <label for="history-comorbidities-status">Сопутствующие состояния</label>
          <select
            id="history-comorbidities-status"
            bind:value={medicalHistory.comorbidities_status}
          >
            {#each Object.entries(comorbidityLabels) as [value, label]}
              <option {value}>{label}</option>
            {/each}
          </select>
          {#if medicalHistory.comorbidities_status === 'present'}
            <label for="history-comorbidities-details">Описание сопутствующих состояний</label>
            <textarea
              id="history-comorbidities-details"
              rows="3"
              maxlength="6000"
              required
              bind:value={medicalHistory.comorbidities_details}></textarea>
          {/if}
          <p class="muted small-text">
            «Нет сведений» не означает подтверждённого отсутствия состояний. При выборе «Не
            выявлены» или «Нет сведений» сохранённое описание будет очищено.
          </p>
        </fieldset>
        <p class="muted small-text">
          Сведения карточки и анамнез доступны пациенту. Для внутренних наблюдений используйте
          заметку врача. Изменения карточки сохраняются в истории версий.
        </p>
      {/if}
      {#if error}<div class="feedback error" role="alert">{error}</div>{/if}
      {#if stale}<button class="secondary wide" type="button" disabled={busy} onclick={reload}
          >Загрузить актуальную карточку</button
        >
        <p class="muted small-text">Это заменит несохранённые поля данными с сервера.</p>{/if}
      <div class="modal-actions">
        <button class="secondary" type="button" disabled={busy} onclick={onclose}>Отмена</button
        ><button class="primary" type="submit" disabled={busy || stale}
          >{busy
            ? 'Сохраняем'
            : stage === 'create'
              ? 'Создать приглашение'
              : 'Сохранить карточку'}<Check size={16} /></button
        >
      </div>
    </form>
  {/if}
</dialog>
