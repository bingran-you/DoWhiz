export { getNextThemeSwitch, getThemeForLocalTime } from '../../theme/localTheme';

export const shouldEnableMouseField = () => {
  if (typeof window === 'undefined') {
    return false;
  }

  const matchMedia = window.matchMedia
    ? (query) => window.matchMedia(query).matches
    : () => false;

  const prefersReducedMotion = matchMedia('(prefers-reduced-motion: reduce)');
  const prefersReducedData = matchMedia('(prefers-reduced-data: reduce)');
  const smallScreen = matchMedia('(max-width: 768px)');

  const connection =
    typeof navigator !== 'undefined'
      ? navigator.connection || navigator.mozConnection || navigator.webkitConnection
      : null;
  const saveData = connection?.saveData;
  const slowConnection = connection?.effectiveType
    ? ['slow-2g', '2g', '3g'].includes(connection.effectiveType)
    : false;

  return !(prefersReducedMotion || prefersReducedData || smallScreen || saveData || slowConnection);
};
