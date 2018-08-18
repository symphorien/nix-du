{nix, fetchpatch, lib}:
# this function patches nix to fix an issue causing spurious test failures
if lib.versionAtLeast nix.version "2.1pre6337" then nix else
nix.overrideAttrs(old: {
  doInstallCheck = false;
  patches = [
    (fetchpatch {
    url = https://patch-diff.githubusercontent.com/raw/NixOS/nix/pull/2223.patch;
    sha256 = "0ykmqas4qsqj3xhb26x8i9711k5sb0x84g779ms3zp083ckpdbhf";
    })
    (fetchpatch {
    url = https://patch-diff.githubusercontent.com/raw/NixOS/nix/pull/2234.patch;
    sha256 = "0pwwx4833f16082rddp2jbvcra3avnggci1c0v7fr72glsvm6vrb";
    })
  ];
})
