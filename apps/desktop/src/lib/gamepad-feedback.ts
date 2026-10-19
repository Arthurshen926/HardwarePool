import type { UiRoute } from './types';

// This Source belongs exclusively to the single local DS4/Xbox projection.
// Route state, not a page variable, survives WebView recreation.
export const GAMEPAD_FEEDBACK_SOURCE = 'd41251b3-63ed-4e83-807e-4c4ab9b20b02';
export function ownedGamepadFeedback(routes: UiRoute[]): UiRoute[] {
  return routes.filter(route => route.sourceCapabilityId === GAMEPAD_FEEDBACK_SOURCE
    && route.profile === 'capyio.haptics.feedback/1'
    && (['prepared', 'starting', 'active', 'stopping'].includes(route.state)
      || (route.state === 'draft' && route.authorization === 'pending')));
}
