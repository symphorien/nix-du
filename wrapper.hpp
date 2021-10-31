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

#ifndef NIXVER
#define NIXVER 204
#endif

#if NIXVER >= 203
#define findroots(store) store->findRoots(false)
#else
#define findroots(store) store->findRoots()
#endif

#if NIXVER >= 204
#define PATH StorePath
#else
#define PATH Path
#endif

#if NIXVER >= 204
// ->deriver is optional<storepath>
#define DERIVER_IS_EMPTY(d) (!d.has_value())
#define DERIVER_GET(d) d.value()
#else
// ->deriver is path, aka string
#define DERIVER_IS_EMPTY(d) d.empty()
#define DERIVER_GET(d) d
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

      std::unordered_map<PATH, Info> node_to_id;
      // Registers the node if it was not already registered, and return its path info
      // Returns: pair of a boolean indicating if it was already visited, and path info
      auto get_infos = [&] (const PATH& p) {
        auto it = node_to_id.find(p);
        if (it==node_to_id.end()) {
          Info info = {
            store->queryPathInfo(p).get_ptr(), //data
            (unsigned)(node_to_id.size()), // index
          };
          path_t entry;
          entry.is_root = 0;
          entry.size = info.data->narSize;
#if NIXVER >= 204
          std::string path = store->storeDir + "/";
          path.append(p.to_string());
#else
          std::string path = info.data->path;
#endif
          entry.path = path.c_str();
          node_to_id[p] = info;
          register_node(graph, &entry);
          return std::make_pair(false, info);
        } else {
          return std::make_pair(true, it->second);
        }
      };

      // queue for graph traversal
      std::vector<PATH> queue;
      // initialise with either all nodes or just the root we want
      if (!rootPath) {
        // dump all the store
        std::set<PATH> paths = store->queryAllValidPaths();
        std::copy(paths.begin(), paths.end(), std::back_inserter(queue));
      } else {
        // dump only the recursive closure of rootPath
#if NIXVER >= 204
        const PATH rootDrv = store->followLinksToStorePath(rootPath);
#else
        const Path naiveRootPath(rootPath);
        const PATH rootDrv = store->followLinksToStorePath(naiveRootPath);
#endif
        if (!store->isValidPath(rootDrv)) {
          throw Error("'%s' is not a valid path", rootPath);
        }
        queue.push_back(rootDrv);
      }

      // follow references in graph traversal, register corresponding edges
      while (!queue.empty()) {
        PATH path = queue.back();
        queue.pop_back();
        Info from = get_infos(path).second;
        // register edges to references
        for (const PATH& dep: from.data->references) {
          Info to; bool cached;
          std::tie(cached, to) = get_infos(dep);
          register_edge(graph, from.index, to.index);
          if (!cached) {
            queue.push_back(dep);
          }
        }
        // register edges from/to drv if this path has a derivation
        if ((settings.gcKeepOutputs || settings.gcKeepDerivations) && (!DERIVER_IS_EMPTY(from.data->deriver)) && store->isValidPath(DERIVER_GET(from.data->deriver))) {
          Info drv; bool drv_was_cached;
          std::tie(drv_was_cached, drv) = get_infos(DERIVER_GET(from.data->deriver));
          if (settings.gcKeepDerivations) {
            register_edge(graph, from.index, drv.index);
          }
          if (settings.gcKeepOutputs) {
            register_edge(graph, drv.index, from.index);
          }
          if (!drv_was_cached) {
            queue.push_back(DERIVER_GET(from.data->deriver));
          }
        }
      }

      if (!rootPath) {
        // register roots and add edge to corresponding store path
        unsigned index = node_to_id.size();
#if NIXVER >= 203
        for (auto &[storepath, links] : findroots(store)) {
        for (auto link: links) {
#else
        for (auto root : findroots(store)) {{
          PATH link, storepath;
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
#if NIXVER >= 204
    restoreProcessContext();
#else
    restoreSignals();
#endif
    return retcode;
  }
}


