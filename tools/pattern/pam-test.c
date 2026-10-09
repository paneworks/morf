/* Test transport for a temporary pam_start_confdir stack, never system PAM. */
#define _GNU_SOURCE
#include <security/pam_appl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int conversation(int count, const struct pam_message **messages,
                        struct pam_response **responses, void *data) {
    struct pam_response *reply = calloc((size_t)count, sizeof(*reply));
    if (!reply) return PAM_BUF_ERR;
    for (int i = 0; i < count; ++i) {
        if (messages[i]->msg_style == PAM_PROMPT_ECHO_OFF || messages[i]->msg_style == PAM_PROMPT_ECHO_ON) {
            reply[i].resp = strdup(data);
            if (!reply[i].resp) {
                for (int j = 0; j < i; ++j) free(reply[j].resp);
                free(reply); return PAM_BUF_ERR;
            }
        }
    }
    *responses = reply;
    return PAM_SUCCESS;
}

int main(int argc, char **argv) {
    if (argc != 3) return 2;
    char secret[80];
    if (!fgets(secret, sizeof(secret), stdin)) return 2;
    secret[strcspn(secret, "\n")] = 0;
    struct pam_conv conv = {conversation, secret};
    pam_handle_t *pam = NULL;
    int status = pam_start_confdir("test-pattern", argv[2], &conv, argv[1], &pam);
    if (status == PAM_SUCCESS) status = pam_authenticate(pam, 0);
    if (status == PAM_SUCCESS) status = pam_acct_mgmt(pam, 0);
    if (pam) pam_end(pam, status);
    explicit_bzero(secret, sizeof(secret));
    return status == PAM_SUCCESS ? 0 : 1;
}
