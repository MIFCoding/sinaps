export type PullPhase = 'idle' | 'pulling' | 'refreshing' | 'returning';
export const PULL_INDICATOR_THRESHOLD = 44;
export const PULL_REFRESH_THRESHOLD = 76;

type Options = {
  disabled?: boolean;
  onrefresh: () => Promise<void>;
  onchange: (distance: number, busy: boolean, phase: PullPhase) => void;
};

export function pullToRefresh(node: HTMLElement, initial: Options) {
  let options = initial;
  let startX = 0,
    startY = 0,
    distance = 0,
    target = 0;
  let motionVersion = 0;
  let tracking = false,
    refreshing = false,
    alive = true;
  let phase: PullPhase = 'idle';
  let frame = 0,
    previousTime = 0,
    holdTimer = 0;
  let finishAnimation: (() => void) | undefined;
  let finishHold: (() => void) | undefined;
  const reducedMotion = () => window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  const emit = () => {
    if (alive) options.onchange(distance, refreshing, phase);
  };

  function stopFrame() {
    cancelAnimationFrame(frame);
    frame = 0;
    previousTime = 0;
    const finish = finishAnimation;
    finishAnimation = undefined;
    finish?.();
  }
  function follow(time: number) {
    frame = 0;
    if (!alive || !tracking) return;
    const elapsed = Math.min(48, previousTime ? time - previousTime : 16);
    previousTime = time;
    // Сглаживание зависит от времени кадра, поэтому скорость экрана не меняет ощущение жеста.
    distance += (target - distance) * (1 - Math.exp(-elapsed / 28));
    if (Math.abs(target - distance) < 0.1) distance = target;
    emit();
    if (distance !== target) frame = requestAnimationFrame(follow);
    else previousTime = 0;
  }
  function animate(to: number, milliseconds: number): Promise<void> {
    stopFrame();
    if (!alive) return Promise.resolve();
    if (reducedMotion() || document.hidden || Math.abs(distance - to) < 0.1) {
      distance = to;
      emit();
      return Promise.resolve();
    }
    const from = distance,
      started = performance.now();
    return new Promise((resolve) => {
      finishAnimation = resolve;
      const step = (time: number) => {
        if (!alive) return;
        const progress = Math.min(1, (time - started) / milliseconds);
        const eased = (1 - Math.cos(Math.PI * progress)) / 2;
        distance = from + (to - from) * eased;
        emit();
        if (progress < 1) frame = requestAnimationFrame(step);
        else {
          frame = 0;
          finishAnimation = undefined;
          resolve();
        }
      };
      frame = requestAnimationFrame(step);
    });
  }
  function hold(): Promise<void> {
    if (reducedMotion() || document.hidden) return Promise.resolve();
    return new Promise((resolve) => {
      finishHold = resolve;
      holdTimer = window.setTimeout(() => {
        holdTimer = 0;
        finishHold = undefined;
        resolve();
      }, 280);
    });
  }
  async function restore() {
    const version = ++motionVersion;
    phase = 'returning';
    await animate(0, 340);
    if (!alive || version !== motionVersion) return;
    distance = 0;
    target = 0;
    refreshing = false;
    phase = 'idle';
    emit();
  }
  function cancel() {
    if (!tracking) return;
    tracking = false;
    void restore();
  }
  function start(event: TouchEvent) {
    if (refreshing || phase === 'returning') return;
    const element = event.target instanceof Element ? event.target : null;
    if (
      options.disabled ||
      event.touches.length !== 1 ||
      window.scrollY > 0 ||
      document.querySelector('dialog[open]') ||
      element?.closest('button, input, textarea, select, a')
    ) {
      cancel();
      return;
    }
    motionVersion++;
    stopFrame();
    target = distance;
    startX = event.touches[0].clientX;
    startY = event.touches[0].clientY;
    phase = 'pulling';
    tracking = true;
    emit();
  }
  function move(event: TouchEvent) {
    if (!tracking) return;
    if (event.touches.length !== 1 || options.disabled || window.scrollY > 0) {
      cancel();
      return;
    }
    const y = event.touches[0].clientY - startY;
    const x = Math.abs(event.touches[0].clientX - startX);
    if (y < 0 || x > Math.max(12, y)) {
      cancel();
      return;
    }
    if (y < 12) return;
    event.preventDefault();
    // Порог распознавания не даёт скачка, а сопротивление постепенно ограничивает вытягивание.
    target = 108 * (1 - Math.exp(-(y - 12) / 180));
    if (reducedMotion()) {
      distance = target;
      emit();
    } else if (!frame) frame = requestAnimationFrame(follow);
  }
  async function end() {
    if (!tracking) return;
    tracking = false;
    if (target < PULL_REFRESH_THRESHOLD || options.disabled) {
      await restore();
      return;
    }
    refreshing = true;
    phase = 'refreshing';
    // Запрос начинается сразу; минимальное время касается только видимости индикатора.
    try {
      await Promise.all([options.onrefresh(), animate(PULL_REFRESH_THRESHOLD, 160), hold()]);
    } finally {
      await restore();
    }
  }
  node.addEventListener('touchstart', start, { passive: true });
  node.addEventListener('touchmove', move, { passive: false });
  node.addEventListener('touchend', end);
  node.addEventListener('touchcancel', cancel);
  emit();
  return {
    update(next: Options) {
      options = next;
    },
    destroy() {
      alive = false;
      tracking = false;
      stopFrame();
      clearTimeout(holdTimer);
      finishHold?.();
      finishHold = undefined;
      node.removeEventListener('touchstart', start);
      node.removeEventListener('touchmove', move);
      node.removeEventListener('touchend', end);
      node.removeEventListener('touchcancel', cancel);
    },
  };
}
