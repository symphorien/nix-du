// SPDX-License-Identifier: LGPL-3.0

#include <cstdint>

extern "C" {
typedef struct
{
  const char * path;
  uint64_t size;
  int is_root;
} path_t;
int populateGraph(void * graph, const char * rootPath);
}
