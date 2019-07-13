// SPDX-License-Identifier: LGPL-3.0

/* this includes fixes
   /nix/store/cqhdk51xqxj1990v20y3wfnvhr0r8yds-nix-1.11.15-dev/include/nix/util.hh:362:24: error: implicit instantiation of undefined template 'std::__cxx11::basic_istringstream<char, std::char_traits<char>, std::allocator<char> >'
   /nix/store/c30dlkmiyrjxxjv6nv63igjkzcj1fzxi-gcc-6.4.0/include/c++/6.4.0/iosfwd:100:11: note: template is declared here
*/
#include <sstream>

#include <iostream>
#include <unordered_map>
#include <nix/config.h> // #define SYSTEM
#include <nix/util.hh> // restoreSignals
#include <nix/shared.hh> // initNix
#include <nix/local-store.hh>
#include <nix/remote-store.hh>

// In nix 2.3, Roots is a map from a string to a set of string instead of a map of string to string
#ifndef ROOTS_ARE_MAP_TO_SET
#define ROOTS_ARE_MAP_TO_SET 0
#endif

// In nix 2.3, store->findRoots gained a new argument, censor
#ifndef FINDROOTS_HAS_CENSOR
#define FINDROOTS_HAS_CENSOR 0
#endif

#if FINDROOTS_HAS_CENSOR
#define findroots(store) store->findRoots(false)
#else
#define findroots(store) store->findRoots()
#endif

extern "C" {
  typedef struct {
    const char* path;
    uint64_t size;
    int is_root;
  } path_t;
  typedef struct {
    std::shared_ptr<const nix::ValidPathInfo> data;
    unsigned index;
  } Info;
  extern void register_node(void *graph, path_t *node);
  extern void register_edge(void *graph, unsigned from, unsigned to);
  int populateGraph(void *graph, const char* rootPath) {
    using namespace nix;
    int retcode = handleExceptions("nix-du", [graph, rootPath]() {
      initNix();
      auto store = openStore();

      std::unordered_map<Path, Info> node_to_id;
      auto get_infos = [&] (const Path& p) {
        auto it = node_to_id.find(p);
        if (it==node_to_id.end()) {
          Info info = {
            store->queryPathInfo(p).get_ptr(), //data
            (unsigned)(node_to_id.size()), // index
          };
          path_t entry;
          entry.is_root = 0;
          entry.size = info.data->narSize;
          entry.path = info.data->path.c_str();
          node_to_id[p] = info;
          register_node(graph, &entry);
          return std::make_pair(false, info);
        } else {
          return std::make_pair(true, it->second);
        }
      };

      std::vector<Path> queue;
      if (!rootPath) {
        // dump all the store
        std::set<Path> paths = store->queryAllValidPaths();
        std::copy(paths.begin(), paths.end(), std::back_inserter(queue));
      } else {
        // dump only the recursive closure of rootPath
        const Path naiveRootPath(rootPath);
        const Path rootDrv = store->followLinksToStorePath(naiveRootPath);
        if (!store->isValidPath(rootDrv)) {
          throw Error("'%s' is not a valid path", rootPath);
        }
        queue.push_back(rootDrv);
      }

      while (!queue.empty()) {
        Path path = queue.back();
        queue.pop_back();
        Info from = get_infos(path).second;
        for (const Path& dep: from.data->references) {
          Info to; bool cached;
          std::tie(cached, to) = get_infos(dep);
          register_edge(graph, from.index, to.index);
          if (!cached) {
            queue.push_back(dep);
          }
        }
      }

      if (!rootPath) {
        unsigned index = node_to_id.size();
#if ROOTS_ARE_MAP_TO_SET
        for (auto &[storepath, links] : findroots(store)) {
        for (auto link: links) {
#else
        for (auto root : findroots(store)) {{
          Path link, storepath;
          std::tie(link, storepath) = root;
#endif
          if (store->isValidPath(storepath)) {
            path_t entry;
            entry.is_root = 1;
            entry.size = link.size();
            entry.path = link.c_str();
            register_node(graph, &entry);
            Info to = get_infos(storepath).second;
            register_edge(graph, index, to.index);
            ++index;
          }
        }
        }
      }
    });
    restoreSignals();
    return retcode;
  }
}


