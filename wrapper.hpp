/* this includes fixes
 /nix/store/cqhdk51xqxj1990v20y3wfnvhr0r8yds-nix-1.11.15-dev/include/nix/util.hh:362:24: error: implicit instantiation of undefined template 'std::__cxx11::basic_istringstream<char, std::char_traits<char>, std::allocator<char> >'
/nix/store/c30dlkmiyrjxxjv6nv63igjkzcj1fzxi-gcc-6.4.0/include/c++/6.4.0/iosfwd:100:11: note: template is declared here
*/

#include <sstream>
#include <iostream>
#include <nix/shared.hh> // initNix
#include <nix/local-store.hh>
#include <nix/remote-store.hh>

namespace nix_adapter {
  using namespace nix;

  typedef PathSet::iterator PathSetIterator;

  const char* path_to_c_str(const Path& p) {
    return p.c_str();
  }

  PathSet::iterator begin_path_set(const PathSet ps) {
    return ps.begin();
  }

  size_t size_path_set(const PathSet ps) {
    return ps.size();
  }

  // bindgen replaces PathSet::iterator by u8 which is Copy.
  // we cannot mutate it.
  PathSet::iterator inc_path_set_it(PathSet::iterator it) {
    it++;
    return it;
  }

  /*
  bool is_end_path_set_it(const PathSet& ps, const PathSet::iterator it) {
    return it == ps.end();
  }
  */

  Path dereference_path_set_it(const PathSet::iterator it) {
    return *it;
  }
}
