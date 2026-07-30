// Join class names, ignoring falsy values. Supports strings and nested arrays,
// e.g. cn('badge', isActive && 'badge--active', className).
export function cn(...inputs) {
  return inputs.flat(Infinity).filter(Boolean).join(' ');
}
