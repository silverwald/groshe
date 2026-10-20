#ifndef GROSHE_IOS_H
#define GROSHE_IOS_H

#ifdef __cplusplus
extern "C" {
#endif

#include <stdint.h>
#include <stdbool.h>

/**
 * Initializes the Groshe runtime with the specified sandbox storage directory.
 * @param storage_path Null-terminated UTF-8 string containing the path to Documents or Application Support directory.
 * @return 0 on success, non-zero error code on failure.
 */
int32_t groshe_ios_init(const char *storage_path);

/**
 * Starts the Slint UI event loop. Blocks until the window is closed.
 * @return 0 on normal exit, non-zero error code on failure.
 */
int32_t groshe_ios_run(void);

/**
 * Lifecycle hook called by iOS AppDelegate/SceneDelegate when the app transitions into the foreground.
 * Wakes up sync polling, gossip presence, and connection retry timers.
 * @return 0 on success, non-zero error code on failure.
 */
int32_t groshe_ios_on_foreground(void);

/**
 * Lifecycle hook called by iOS AppDelegate/SceneDelegate when the app transitions into the background.
 * Flushes dirty store state and pauses active network streams to adhere to iOS 30-second budget.
 * @return 0 on success, non-zero error code on failure.
 */
int32_t groshe_ios_on_background(void);

/**
 * Shuts down the Groshe runtime, flushes pending database writes, and releases background resources.
 * @return 0 on success, non-zero error code on failure.
 */
int32_t groshe_ios_cleanup(void);

/**
 * Queries the current lifecycle state of the runtime (0 = uninitialized, 1 = active/foreground, 2 = paused/background).
 */
int32_t groshe_ios_get_state(void);

#ifdef __cplusplus
}
#endif

#endif /* GROSHE_IOS_H */
