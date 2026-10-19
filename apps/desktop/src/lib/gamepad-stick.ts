export type StickMode = 'fixed' | 'floating' | 'follow';
export interface StickGeometry { x: number; y: number; radius: number }

// Presentation coordinates only; emitted axes keep the existing input contract.
export function stickAxes(center: StickGeometry, x: number, y: number, follow: boolean) {
  let dx = x - center.x;
  let dy = y - center.y;
  const distance = Math.hypot(dx, dy);
  if (follow && distance > center.radius) {
    center.x += dx * (1 - center.radius / distance);
    center.y += dy * (1 - center.radius / distance);
    dx = x - center.x;
    dy = y - center.y;
  }
  const magnitude = Math.hypot(dx, dy) / center.radius;
  const amount = Math.max(0, (Math.min(1, magnitude) - .08) / .92);
  const length = Math.hypot(dx, dy) || 1;
  return { x: Math.round(dx / length * amount * 32767) || 0, y: Math.round(-dy / length * amount * 32767) || 0 };
}
