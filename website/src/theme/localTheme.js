export const DAY_THEME_START_HOUR = 7;
export const NIGHT_THEME_START_HOUR = 19;
export const LOCAL_THEME_CHANGE_EVENT = 'dw:local-theme-change';

export const THEME_META_COLORS = {
  light: '#eef1f5',
  dark: '#0b0d10'
};

export const getThemeForLocalTime = (date = new Date()) => {
  const hour = date.getHours();
  return hour >= DAY_THEME_START_HOUR && hour < NIGHT_THEME_START_HOUR ? 'light' : 'dark';
};

export const getNextThemeSwitch = (date = new Date()) => {
  const next = new Date(date.getTime());
  const hour = date.getHours();

  if (hour < DAY_THEME_START_HOUR) {
    next.setHours(DAY_THEME_START_HOUR, 0, 0, 0);
    return next;
  }

  if (hour < NIGHT_THEME_START_HOUR) {
    next.setHours(NIGHT_THEME_START_HOUR, 0, 0, 0);
    return next;
  }

  next.setDate(next.getDate() + 1);
  next.setHours(DAY_THEME_START_HOUR, 0, 0, 0);
  return next;
};

export const applyTheme = (theme, root = document.documentElement) => {
  root?.setAttribute('data-theme', theme);

  if (typeof window !== 'undefined') {
    window.dispatchEvent(
      new CustomEvent(LOCAL_THEME_CHANGE_EVENT, {
        detail: { theme }
      })
    );
  }

  return theme;
};

export const applyLocalTimeTheme = (root = document.documentElement, date = new Date()) => {
  return applyTheme(getThemeForLocalTime(date), root);
};

export const scheduleLocalTimeTheme = (root = document.documentElement) => {
  if (typeof window === 'undefined') {
    return () => {};
  }

  let timeoutId;

  const scheduleNextSwitch = () => {
    const now = new Date();
    const nextSwitch = getNextThemeSwitch(now);
    const delay = Math.max(nextSwitch.getTime() - now.getTime(), 0);

    timeoutId = window.setTimeout(() => {
      applyLocalTimeTheme(root);
      scheduleNextSwitch();
    }, delay);
  };

  applyLocalTimeTheme(root);
  scheduleNextSwitch();

  return () => {
    if (timeoutId) {
      window.clearTimeout(timeoutId);
    }
  };
};
