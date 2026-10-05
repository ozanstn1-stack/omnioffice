import { useId } from "react";

/**
 * The OmniOffice brand mark: a rounded gradient tile with a white "O" ring and
 * a sparkle. Mirrors `scripts/make-icon.ps1` (the OS icon) so the app icon and
 * the in-app brand stay the same design.
 */
export function BrandMark({ size = 36, className }: { size?: number; className?: string }) {
  const id = useId();
  const tile = `${id}-tile`;
  const ring = `${id}-ring`;

  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 64 64"
      className={className}
      role="img"
      aria-hidden="true"
      focusable="false"
    >
      <defs>
        <linearGradient id={tile} x1="6" y1="4" x2="58" y2="60" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#4F46E5" />
          <stop offset="0.45" stopColor="#6366F1" />
          <stop offset="0.8" stopColor="#7C3AED" />
          <stop offset="1" stopColor="#9333EA" />
        </linearGradient>
        <linearGradient id={ring} x1="32" y1="18" x2="32" y2="46" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="#FFFFFF" />
          <stop offset="1" stopColor="#E0E7FF" />
        </linearGradient>
      </defs>
      <rect x="2" y="2" width="60" height="60" rx="15" fill={`url(#${tile})`} />
      <rect
        x="2.6"
        y="2.6"
        width="58.8"
        height="58.8"
        rx="14.4"
        fill="none"
        stroke="#FFFFFF"
        strokeOpacity="0.22"
        strokeWidth="1.2"
      />
      <circle cx="32" cy="31.6" r="13.5" fill="none" stroke={`url(#${ring})`} strokeWidth="6" />
      <path
        d="M48.9 10.6 L50.7 14.7 L54.8 16.5 L50.7 18.3 L48.9 22.4 L47.1 18.3 L43 16.5 L47.1 14.7 Z"
        fill="#FFFFFF"
      />
      <circle cx="55" cy="9.6" r="1.7" fill="#FFFFFF" fillOpacity="0.75" />
    </svg>
  );
}
