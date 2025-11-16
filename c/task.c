#include <stddef.h>
#include <stdio.h>
#include <stdlib.h>

// clang -o c/bin/task c/task.c && c/bin/task

typedef struct {
  void* raw;
} Task;

typedef struct {
  void (*schedule)();
  void (*poll)();
} VTable;

typedef struct {
  size_t state;
  VTable* vtable;
} Header;

void schedule() {
  printf("schedule\n");
}

void poll() {
  printf("poll\n");
}

VTable vtable = {
  .schedule = schedule,
  .poll = poll,
};

typedef struct {
  Header header;
} RawTask;

int main() {
  RawTask* raw = malloc(sizeof(RawTask));
  raw->header.state = 32;
  raw->header.vtable = &vtable;

  Task task = {.raw = raw};

  printf("task.raw = %p raw = %p raw.header = %p\n", task.raw, raw, &raw->header);

  Header* header = (Header*)task.raw;

  (header->vtable->schedule)();
  (header->vtable->poll)();
  printf("state = %zu\n", header->state);

  return 0;
}
