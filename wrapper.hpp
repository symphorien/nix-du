// SPDX-License-Identifier: LGPL-3.0

/* this includes fixes
   /nix/store/cqhdk51xqxj1990v20y3wfnvhr0r8yds-nix-1.11.15-dev/include/nix/util.hh:362:24: error: implicit instantiation of undefined template 'std::__cxx11::basic_istringstream<char, std::char_traits<char>, std::allocator<char> >'
   /nix/store/c30dlkmiyrjxxjv6nv63igjkzcj1fzxi-gcc-6.4.0/include/c++/6.4.0/iosfwd:100:11: note: template is declared here
*/

#include <sstream>

#include <iostream>
#include <unordered_map>
#include <nix/shared.hh> // initNix
#include <nix/local-store.hh>
#include <nix/remote-store.hh>

extern "C" {
  typedef struct {
    int is_root;
    const char* path;
    uint64_t size;
  } path_t;
  typedef struct {
    unsigned index;
    nix::ValidPathInfo data;
  } Info;
  extern void register_node(void *graph, path_t *node);
  extern void register_edge(void *graph, unsigned from, unsigned to);
  void populateGraph(void *graph) {
    using namespace nix;
    initNix();
    auto store = openStore();

    std::unordered_map<Path, Info> node_to_id;
    auto get_infos = [&] (const Path& p) {
      auto it = node_to_id.find(p);
      if (it==node_to_id.end()) {
        Info info = {
          (unsigned)(node_to_id.size()), // index
          store->queryPathInfo(p), //data
        };
        path_t entry;
        entry.is_root = 0;
        entry.size = info.data.narSize;
        entry.path = info.data.path.c_str();
        node_to_id[p] = info;
        register_node(graph, &entry);
        return info;
      } else {
        return it->second;
      }
    };

    std::set<Path> paths = store->queryAllValidPaths();

    for (const Path& path: paths) {
      Info from = get_infos(path);
      for (const Path& dep: from.data.references) {
        Info to = get_infos(dep);
        register_edge(graph, from.index, to.index);
      }
    }

    unsigned index = node_to_id.size();
    for (auto root : store->findRoots()) {
      Path link, storepath;
      std::tie(link, storepath) = root;
      path_t entry;
      entry.is_root = 1;
      entry.size = 1;
      entry.path = link.c_str();
      register_node(graph, &entry);
      Info to = get_infos(storepath);
      register_edge(graph, index, to.index);
      ++index;
    }
  }
}


