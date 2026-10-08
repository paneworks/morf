/* Root-only pattern storage; the setuid entry point only verifies credentials.
 * Build with libxcrypt. Input is stdin, never argv, environment or a log. */
#define _GNU_SOURCE
#include <crypt.h>
#include <errno.h>
#include <fcntl.h>
#include <pwd.h>
#include <signal.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>

#ifndef PATTERN_PARENT
#define PATTERN_PARENT "/var/lib/morf"
#endif
#ifndef PATTERN_STATE
#define PATTERN_STATE PATTERN_PARENT "/pattern"
#endif
#ifndef PATTERN_PUBLIC_PARENT
#define PATTERN_PUBLIC_PARENT "/etc/morf"
#endif
#ifndef PATTERN_PUBLIC
#define PATTERN_PUBLIC PATTERN_PUBLIC_PARENT "/pattern"
#endif

/* The same adjacency rule is used by lib.util.pattern at the UI boundary. */
static const char *invalid_pattern(const char *s) {
    size_t n = strlen(s);
    if (n < 4 || n > 9) return "Connect four to nine dots.";
    unsigned seen = 0;
    int previous = -1;
    for (size_t i = 0; i < n; ++i) {
        int dot = s[i] - '1';
        if (dot < 0 || dot > 8) return "Use only the dots 1 to 9.";
        if (seen & (1u << dot)) return "Each dot can be used only once.";
        if (previous >= 0 && (abs(dot / 3 - previous / 3) > 1 || abs(dot % 3 - previous % 3) > 1))
            return "Connect neighbouring dots; diagonals are allowed.";
        seen |= 1u << dot;
        previous = dot;
    }
    return NULL;
}

static bool read_secret(char *out, size_t capacity) {
    size_t n = 0;
    bool ended = false;
    for (;;) {
        char c;
        ssize_t got = read(STDIN_FILENO, &c, 1);
        if (got < 0 && errno == EINTR) continue;
        if (got < 0) return false;
        if (!got) { out[n] = 0; return true; }
        if (c == '\n' || c == '\0') {
            if (ended) return false;
            ended = true;
        } else {
            if (ended || n + 1 >= capacity) return false;
            out[n++] = c;
        }
    }
}

static bool safe_file(int fd) {
    struct stat s;
    return fd >= 0 && fstat(fd, &s) == 0 && S_ISREG(s.st_mode)
        && s.st_uid == 0 && s.st_nlink == 1 && !(s.st_mode & 077);
}

static int directory(const char *path, mode_t mode, bool create) {
    bool made = false;
    if (create) {
        if (!mkdir(path, mode)) made = true;
        else if (errno != EEXIST) return -1;
    }
    int fd = open(path, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    struct stat s;
    if (fd < 0) return -1;
    if (made && fchmod(fd, mode)) { close(fd); return -1; }
    if (fstat(fd, &s) || s.st_uid != 0 || (s.st_mode & 022)
        || (mode == 0700 && (s.st_mode & 077))) { close(fd); return -1; }
    return fd;
}

static bool write_all(int fd, const char *data, size_t n) {
    while (n) {
        ssize_t wrote = write(fd, data, n);
        if (wrote < 0 && errno == EINTR) continue;
        if (wrote <= 0) return false;
        data += wrote; n -= (size_t)wrote;
    }
    return true;
}

static bool replace_file(int dir, const char *name, const char *data, mode_t mode) {
    char tmp[80];
    snprintf(tmp, sizeof(tmp), ".new-%ld", (long)getpid());
    int fd = openat(dir, tmp, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, 0600);
    if (fd < 0) return false;
    bool ok = write_all(fd, data, strlen(data)) && fchmod(fd, mode) == 0 && fsync(fd) == 0;
    if (close(fd)) ok = false;
    if (ok) ok = renameat(dir, tmp, dir, name) == 0;
    if (!ok) unlinkat(dir, tmp, 0);
    return ok && fsync(dir) == 0;
}

static bool read_file(int dir, const char *name, char *out, size_t cap) {
    int fd = openat(dir, name, O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
    if (!safe_file(fd)) { if (fd >= 0) close(fd); return false; }
    ssize_t n = read(fd, out, cap);
    close(fd);
    if (n <= 0 || (size_t)n >= cap) return false;
    out[n] = 0;
    return true;
}

static bool same_hash(const char *a, const char *b) {
    size_t n = strlen(a);
    if (strlen(b) != n) return false;
    unsigned different = 0;
    for (size_t i = 0; i < n; ++i) different |= (unsigned char)a[i] ^ (unsigned char)b[i];
    return different == 0;
}

int main(int argc, char **argv) {
    struct rlimit core = {0, 0};
    if (setrlimit(RLIMIT_CORE, &core)) return 1;
    alarm(10);
    umask(077);
    bool validate = argc == 2 && !strcmp(argv[1], "--validate");
    bool check = argc >= 2 && !strcmp(argv[1], "--check");
    bool set = argc == 3 && !strcmp(argv[1], "--set");
    bool clear = argc == 3 && !strcmp(argv[1], "--clear");
    bool status = argc == 3 && !strcmp(argv[1], "--status");
    if (!validate && !check && !set && !clear && !status) return 1;
    if (check && argc > 3) return 1;
    char secret[11] = {0}, stored[CRYPT_OUTPUT_SIZE] = {0};
    struct crypt_data crypt_state = {0};
    int result = 1, dir = -1, lock = -1, public = -1;
    if (validate || set || check) {
        if (!read_secret(secret, sizeof(secret))) goto done;
        const char *error = invalid_pattern(secret);
        if (error) { if (!check) fprintf(stderr, "%s\n", error); goto done; }
        if (validate) { result = 0; goto done; }
    }
    if (geteuid() != 0 || ((set || clear) && getuid() != 0)) goto done;
    const char *user = argc == 3 ? argv[2] : getenv("PAM_USER");
    if (!user || !*user || strlen(user) > 64 || strspn(user, "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_.-") != strlen(user)) goto done;
    struct passwd *account = getpwnam(user);
    if (!account || account->pw_uid == 0 || (getuid() != 0 && getuid() != account->pw_uid)) goto done;
    char hashname[80], ratename[80], lockname[80];
    snprintf(hashname, sizeof(hashname), "%lu.hash", (unsigned long)account->pw_uid);
    snprintf(ratename, sizeof(ratename), "%lu.attempts", (unsigned long)account->pw_uid);
    snprintf(lockname, sizeof(lockname), "%lu.lock", (unsigned long)account->pw_uid);
    int parent = directory(PATTERN_PARENT, 0755, set);
    if (parent < 0) goto done;
    close(parent);
    dir = directory(PATTERN_STATE, 0700, set);
    if (dir < 0) goto done;
    if (status) { result = read_file(dir, hashname, stored, sizeof(stored)) ? 0 : 1; goto done; }
    lock = openat(dir, lockname, O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK, 0600);
    if (!safe_file(lock) || flock(lock, LOCK_EX | LOCK_NB)) goto done;
    if (set || clear) {
        parent = directory(PATTERN_PUBLIC_PARENT, 0755, true);
        if (parent < 0) goto done;
        close(parent);
        public = directory(PATTERN_PUBLIC, 0755, true);
        if (public < 0) goto done;
        /* Public files are empty enrollment markers, never password hashes. */
        if (clear) {
            if (unlinkat(dir, hashname, 0) && errno != ENOENT) goto done;
            if (unlinkat(public, user, 0) && errno != ENOENT) goto done;
            result = 0; goto done;
        }
        char salt[CRYPT_GENSALT_OUTPUT_SIZE];
        if (!crypt_gensalt_rn("$y$", 5, NULL, 0, salt, sizeof(salt))) goto done;
        char *hashed = crypt_r(secret, salt, &crypt_state);
        if (!hashed || strncmp(hashed, "$y$", 3)) goto done;
        snprintf(stored, sizeof(stored), "%s\n", hashed);
        if (!replace_file(public, user, "", 0644)
            || !replace_file(dir, ratename, "0 0\n", 0600)
            || !replace_file(dir, hashname, stored, 0600)) goto done;
        result = 0; goto done;
    }
    if (!read_file(dir, hashname, stored, sizeof(stored)) || strncmp(stored, "$y$", 3)) goto done;
    stored[strcspn(stored, "\n")] = 0;
    /* Reserve an attempt BEFORE hashing: killing or racing helpers cannot
     * bypass the counter. Password PAM remains available during cooldown. */
    unsigned failures = 0;
    long long until = 0;
    char rate[80];
    if (!read_file(dir, ratename, rate, sizeof(rate))
        || sscanf(rate, "%u %lld", &failures, &until) != 2 || failures > 5 || until < 0) goto done;
    time_t now = time(NULL);
    if (now < 0 || (long long)now < until) goto done;
    if (until) failures = 0;
    ++failures;
    until = failures >= 5 ? (long long)now + 30 : 0;
    snprintf(rate, sizeof(rate), "%u %lld\n", failures, until);
    if (!replace_file(dir, ratename, rate, 0600)) goto done;
    char *hashed = crypt_r(secret, stored, &crypt_state);
    if (hashed && same_hash(hashed, stored) && replace_file(dir, ratename, "0 0\n", 0600)) result = 0;
done:
    explicit_bzero(secret, sizeof(secret));
    explicit_bzero(stored, sizeof(stored));
    explicit_bzero(&crypt_state, sizeof(crypt_state));
    if (public >= 0) close(public);
    if (lock >= 0) close(lock);
    if (dir >= 0) close(dir);
    return result;
}
