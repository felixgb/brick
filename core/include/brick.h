/*
 * Brick mail engine — C interface.
 *
 * The engine owns the local mail store and all network activity. Front ends
 * send JSON requests with brick_call() and receive JSON events through the
 * callback given to brick_open().
 *
 *   request:  {"method": "messages.list", "params": {"folder_id": 3}}
 *   response: {"ok": ...}  or  {"error": "human-readable message"}
 *
 * brick_call() only touches the local database and returns in well under a
 * frame, so it is safe on the UI thread. Network work it starts is reported
 * later as events.
 *
 * Methods
 *   providers.list                                   -> [provider]
 *   settings.get        {key}                        -> string | null
 *   settings.set        {key, value}
 *   accounts.list                                    -> [account]
 *   accounts.add        {provider, email, name, password, ...server overrides}
 *                                                    -> {pending}; then account_added | account_error
 *   accounts.oauth      {provider, email, name}      -> {auth_url}; open it in the browser,
 *                                                       then account_added | account_error
 *   accounts.remove     {id}
 *   folders.list        {account_id}                 -> [folder]
 *   messages.list       {folder_id, offset, limit}   -> {total, offset, rows}
 *                       {role, offset, limit}        the same across every account's folders of that
 *                                                    role; role "inbox" is the unified inbox
 *   messages.search     {query, account_id?, limit?} -> {rows}
 *   messages.get        {id, allow_remote?}          -> {message, cc, body | null}
 *                                                       body null: it is being fetched; wait for body_ready.
 *                                                       body.invitation is true when the email carries a
 *                                                       calendar invitation (see items.import_invitation)
 *   messages.set_flags  {ids, seen?, flagged?}
 *   messages.move       {ids, to: "archive" | "trash" | "junk" | folder_id}
 *   attachments.save    {id, index, path}
 *   send                {account_id, to, cc, bcc, subject, text, reply_to_id?, from?, token}
 *                                                    from: send as one of the account's aliases (setting
 *                                                    aliases_<account id>); anything else is refused
 *                                                    -> then send_done | send_error (with token)
 *   sync.now
 *
 *   To-do and calendar items (one kind of thing; an item with no date is
 *   only a to-do). Dates are "YYYY-MM-DD" and times "HH:MM", local time.
 *   items.list                                       -> [item]
 *                       An item may repeat: "repeat" is "", "daily", "weekly", "monthly" or "yearly",
 *                       and "repeat_until" (day, optional) is the last day an occurrence may start.
 *                       The item's date is its next occurrence still to do; items.set_done on a
 *                       repeating item moves it to the following occurrence instead of finishing it,
 *                       and only finishes it when the series has run out.
 *   items.save          {item}                       -> item   (id 0 or absent creates)
 *   items.set_done      {id, done}
 *   items.delete        {id}
 *   items.for_message   {id}                         -> [item] made from that email
 *   items.source        {id}                         -> {message_id, folder_id} | null
 *   items.export_ics                                 -> text of an iCalendar file with every item
 *   items.import_ics    {text}                       -> {added, skipped, repeats_dropped}
 *                       Items already present (same title, day, start time and end day) are skipped.
 *                       Time zones are ignored (a TZID time is taken as local; UTC is converted to
 *                       local); repeats other than a plain daily/weekly/monthly/yearly are dropped.
 *   items.import_invitation {id}                     -> {added, skipped, repeats_dropped}
 *                       Adds the calendar invitations carried by an email, linked back to it.
 *   items.extract       {id, token}                  -> then items_extracted | items_extract_error
 *                                                       Sends the email to Claude. Nothing is saved;
 *                                                       save the proposals you keep with items.save.
 *   claude.status                                    -> {configured, model, default_model}
 *   claude.set_key      {key}                        (empty removes it; stored in the keychain)
 *
 * Events (field "type")
 *   accounts_changed, account_added {account_id}, account_error {message},
 *   folders_changed {account_id}, messages_changed {account_id, folder_id},
 *   body_ready {id}, body_error {id, message},
 *   sync_started {account_id}, sync_finished {account_id, ok},
 *   send_done {token}, send_error {token, message},
 *   items_changed,
 *   items_extracted {token, message_id, items, summary, truncated},
 *   items_extract_error {token, message_id, message},
 *   error {account_id, message}
 *
 * Settings keys for OAuth: google_client_id, google_client_secret,
 * microsoft_client_id. Claude model: claude_model. Per account (<id> is the
 * account id): aliases_<id> (extra addresses to send as, separated by commas or
 * whitespace) and signature_<id> (used by front ends; the engine only stores it).
 */
#ifndef BRICK_H
#define BRICK_H

#ifdef __cplusplus
extern "C" {
#endif

typedef struct BrickEngine BrickEngine;

/* Called from a background thread. `json` is only valid during the call. */
typedef void (*BrickEventCallback)(const char *json, void *user_data);

/* Returns NULL on failure and, if error_out is non-NULL, stores a message
 * there that must be released with brick_string_free(). */
BrickEngine *brick_open(const char *data_dir, BrickEventCallback callback,
                        void *user_data, char **error_out);

/* Returns a JSON response. Release it with brick_string_free(). */
char *brick_call(BrickEngine *engine, const char *request_json);

void brick_string_free(char *s);

/* Stops background work and closes the store. */
void brick_close(BrickEngine *engine);

#ifdef __cplusplus
}
#endif

#endif
