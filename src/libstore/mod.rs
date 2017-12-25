mod bindings;

use libstore::bindings::*;
use std;
use std::os::raw::c_void;

pub fn init_nix() {
    unsafe { nix_initNix() };
}

#[derive(Debug)]
pub struct Store {
    s: nix_RemoteStore
}

#[derive(Debug)]
pub struct Path {
    pi: nix_ValidPathInfo
}

#[derive(Debug)]
pub struct PathIterator<'a> {
    store: &'a mut Store,
    set: nix_PathSet,
    size: usize,
    cur: usize,
    it: nix_adapter_PathSetIterator // made opaque by bindgen; std::set<std::string>::iterator

}

pub type PathId = std::os::raw::c_ulonglong;

impl<'a> PathIterator<'a> {
    unsafe fn new(store: &'a mut Store, set: nix_PathSet) -> Self {
        let it = nix_adapter_begin_path_set(set);
        let size = nix_adapter_size_path_set(set);
        PathIterator { store, it, size, cur: 0, set }
    }
}

impl Path {
    unsafe fn new_from_ffi(store: &mut Store, path: nix_Path) -> Self {
        let infos = nix_RemoteStore_queryPathInfo(
            &mut store.s as *mut _ as *mut c_void,
            &path as *const _
            );
        // TODO: we are done with this string I guess
        Path { pi : infos }
    }

    pub fn id(&self) -> PathId {
        self.pi.id
    }

    pub fn deps<'a>(&'a self, store: &'a mut Store) -> PathIterator<'a> {
        let set = self.pi.references;
        unsafe { PathIterator::new(store, set) }
    }
}

impl<'a> Iterator for PathIterator<'a> {
    type Item = Path;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur+=1;
        if self.cur > self.size {
            None
        } else {
            let path = unsafe { nix_adapter_dereference_path_set_it(self.it) };
            let p = unsafe { Path::new_from_ffi(self.store, path) };
            self.it = unsafe { nix_adapter_inc_path_set_it(self.it) };
            Some(p)
        }
    }
}

impl Store {
    pub fn new() -> Self {
        unsafe {
            Store { s : nix_RemoteStore::new() }
        }
    }

    pub fn valid_paths(&mut self) -> PathIterator {
        let set = unsafe {
            nix_RemoteStore_queryAllValidPaths(self as *mut _ as *mut c_void)
        };
        unsafe { PathIterator::new(self, set) }
    }
}

