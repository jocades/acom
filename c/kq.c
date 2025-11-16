#include <err.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/event.h>

int main(int argc, char** argv) {
  if (argc < 2) {
    printf("Usage: %s [path]\n", argv[0]);
    return 1;
  }

  int fd = open(argv[1], O_RDONLY);
  if (fd == -1) err(EXIT_FAILURE, "failed to open '%s'", argv[1]);

  int kq = kqueue();
  if (kq == -1) err(EXIT_FAILURE, "kq");

  struct kevent ev = {
    .ident = fd,
    .filter = EVFILT_VNODE,
    .flags = EV_ADD | EV_CLEAR,
    .fflags = NOTE_WRITE,
    .data = 0,
    .udata = NULL,
  };

  int ret = kevent(kq, &ev, 1, NULL, 0, NULL);
  if (ret == -1) err(EXIT_FAILURE, "kevent register");

  struct kevent trigger;

  for (;;) {
    printf("wait...\n");
    int nev = kevent(kq, NULL, 0, &trigger, 1, NULL);
    if (nev == -1) err(EXIT_FAILURE, "kevent wait");

    if (nev > 0) {
      if (trigger.flags & EV_ERROR) {
        errx(EXIT_FAILURE, "event error: %s", strerror(trigger.data));
      } else {
        printf("Something was written in '%s'\n", argv[1]);
      }
    }
  }
}
