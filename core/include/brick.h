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
 *   messages.search     {query, account_id?, limit?} -> {rows}
 *   messages.get        {id, allow_remote?}          -> {message, cc, body | null}
 *                                                       body null: it is being fetched; wait for body_ready
 *   messages.set_flags  {ids, seen?, flagged?}
 *   messages.move       {ids, to: "archive" | "trash" | "junk" | folder_id}
 *   attachments.save    {id, index, path}
 *   send                {account_id, to, cc, bcc, subject, text, reply_to_id?, token}
 *                                                    -> then send_done | send_error (with token)
 *   sync.now
 *
 * Events (field "type")
 *   accounts_changed, account_added {account_id}, account_error {message},
 *   folders_changed {account_id}, messages_changed {account_id, folder_id},
 *   body_ready {id}, body_error {id, message},
 *   sync_started {account_id}, sync_finished {account_id, ok},
 *   send_done {token}, send_error {token, message},
 *   error {account_id, message}
 *
 * Settings keys for OAuth: google_client_id, google_client_secret,
 * microsoft_client_id.
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
