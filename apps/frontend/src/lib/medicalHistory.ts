import type { Patient } from './api';

export const historyFields = [
  ['complaints', 'Жалобы'],
  ['present_illness', 'История текущего состояния'],
  ['life_history', 'Анамнез жизни'],
] as const;

export const comorbidityLabels = {
  unknown: 'Нет сведений',
  none: 'Сопутствующие состояния не выявлены',
  present: 'Есть сведения о сопутствующих состояниях',
} satisfies Record<Patient['medical_history']['comorbidities_status'], string>;
