// SPDX-License-Identifier: LGPL-3.0

mod bindings;

use bindings::*;
use std::os::raw::c_void;
use std::ffi::{CStr, CString};

pub fn init_nix() {
    unsafe { nix_initNix() };
}

#[derive(Debug)]
pub struct Store {
    s: nix_RemoteStore,
}

#[derive(Debug)]
pub struct Path {
    pi: nix_ValidPathInfo,
    path: CString,
}

#[derive(Debug)]
pub struct PathEntry {
    path: nix_Path,
}

#[derive(Debug)]
pub struct PathIterator {
    set: nix_PathSet,
    size: usize,
    cur: usize,
    it: nix_adapter_PathSetIterator,
}

#[derive(Debug)]
pub struct RootsIterator {
    map: nix_Roots,
    size: usize,
    cur: usize,
    it: nix_adapter_RootsIterator,
}

impl PathIterator {
    unsafe fn new(mut set: nix_PathSet) -> Self {
        let it;
        let size;
        {
            let pointer = &mut set as *mut _;
            it = nix_adapter_begin_path_set(pointer);
            size = nix_adapter_size_path_set(pointer);
        }
        PathIterator {
            it,
            size,
            cur: 0,
            set,
        }
    }
}

impl Path {
    unsafe fn new_from_ffi(store: &mut Store, path: nix_Path) -> Self {
        let infos =
            nix_RemoteStore_queryPathInfo(&mut store.s as *mut _ as *mut c_void, &path as *const _);
        let realpath = CStr::from_ptr(nix_adapter_path_to_c_str(&path as *const _));
        Path {
            pi: infos,
            path: realpath.to_owned(),
        }
    }

    pub fn path(&self) -> &CString {
        &self.path
    }

    pub fn deps(&self) -> PathIterator {
        let set = self.pi.references;
        unsafe { PathIterator::new(set) }
    }

    pub fn size(&self) -> u64 {
        self.pi.narSize
    }
}

impl PathEntry {
    pub fn to_path(&self, store: &mut Store) -> Path {
        unsafe { Path::new_from_ffi(store, self.path) }
    }
}

impl Iterator for PathIterator {
    type Item = PathEntry;

    fn next(&mut self) -> Option<Self::Item> {
        self.cur += 1;
        if self.cur > self.size {
            None
        } else {
            let path = unsafe { nix_adapter_dereference_path_set_it(self.it) };
            self.it = unsafe { nix_adapter_inc_path_set_it(self.it) };
            Some(PathEntry { path })
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl ExactSizeIterator for PathIterator {
    fn len(&self) -> usize {
        self.size
    }
}

impl RootsIterator {
    unsafe fn new(mut map: nix_Roots) -> Self {
        let size;
        let it;
        {
            let pointer = &mut map as *mut _;
            it = nix_adapter_begin_roots(pointer);
            size = nix_adapter_size_roots(pointer);
        }
        RootsIterator {
            it,
            size,
            cur: 0,
            map,
        }
    }
}

impl Iterator for RootsIterator {
    type Item = (CString, PathEntry);

    fn next(&mut self) -> Option<Self::Item> {
        self.cur += 1;
        if self.cur > self.size {
            None
        } else {
            let link = unsafe { nix_adapter_dereference_first_roots_it(self.it) };
            let realpath = unsafe { CStr::from_ptr(nix_adapter_path_to_c_str(&link as *const _)) };
            let path = unsafe { nix_adapter_dereference_second_roots_it(self.it) };
            self.it = unsafe { nix_adapter_inc_roots_it(self.it) };
            Some((realpath.to_owned(), PathEntry { path }))
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.size, Some(self.size))
    }
}

impl ExactSizeIterator for RootsIterator {
    fn len(&self) -> usize {
        self.size
    }
}

impl Store {
    pub fn new() -> Self {
        unsafe { Store { s: nix_RemoteStore::new() } }
    }

    pub fn valid_paths(&mut self) -> PathIterator {
        let set = unsafe { nix_RemoteStore_queryAllValidPaths(self as *mut _ as *mut c_void) };
        unsafe { PathIterator::new(set) }
    }

    pub fn roots(&mut self) -> RootsIterator {
        let map = unsafe { nix_RemoteStore_findRoots(self as *mut _ as *mut c_void) };
        unsafe { RootsIterator::new(map) }
    }
}
