/**
 * Whether a protection request can actually bind.
 *
 * A PDF reader verifies the owner password before the user password (ISO 32000-2,
 * 7.6.4.3.2, Algorithm 2.A), so an owner password that is empty or equal to the user
 * password makes whoever opens the file its owner as well - and every permission the
 * file declares becomes advisory. The file format cannot express "restricted but
 * owned by everyone", so the screen has to ask for a separate owner password first.
 */

export interface ProtectionChoice {
  userPassword: string;
  ownerPassword: string;
  allowPrinting: boolean;
  allowCopying: boolean;
  allowEditing: boolean;
  allowCommenting: boolean;
}

/** `null` when the request can bind, otherwise the message key to show. */
export function protectionProblem(choice: ProtectionChoice): string | null {
  const restricted = !(
    choice.allowPrinting &&
    choice.allowCopying &&
    choice.allowEditing &&
    choice.allowCommenting
  );
  if (!restricted) return null;
  if (!choice.ownerPassword) return "security.ownerRequiredForRestrictions";
  if (choice.ownerPassword === choice.userPassword) return "security.ownerMustDiffer";
  return null;
}
