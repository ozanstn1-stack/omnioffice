/**
 * Encrypted person cards (C6/2): typed calls for the local address book.
 *
 * The storage is `src-tauri/src/person_cards.rs` (one sealed file per card);
 * this module only names the calls and the DTOs. Import/export formats live in
 * `person-cards-io.ts`, so neither the dialog nor this file parses text.
 */
import type * as specta from "./generated/specta";
import { call } from "./typed-call";

/** A card as the UI edits it (the id/updatedAt are assigned by the command). */
export type PersonCardInput = specta.PersonCard_Deserialize;
/** A stored card, as the list returns it. */
export type PersonCard = specta.PersonCard_Serialize;
export type PersonCardList = specta.PersonCardList_Serialize;

export const listPersonCards = () => call((c) => c.personCardList());

export const savePersonCard = (card: PersonCardInput) => call((c) => c.personCardSave(card));

/** True when the card was there. */
export const deletePersonCard = (id: string) => call((c) => c.personCardDelete(id));
