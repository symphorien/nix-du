rec {
  shell = "/bin/sh";
  system = builtins.currentSystem;

  join = list: if list==[] then "." else "${builtins.head list}, ${join (builtins.tail list)}";

  mkDerivation = args:
    derivation ({
      inherit system;
      builder = shell;
      args = ["-c" "eval \"$buildCommand\""];
    } // args)
    // { meta = {}; };

  mkNode = name: dependencies:
  (mkDerivation {
    inherit name;
    deps = join dependencies;
    buildCommand = ''
      # ensure deps are seen as dependencies
      echo $deps > $out;
      # ~ 100KB of text
      s10="123456789"
      n10="1 2 3 4 5 6 7 8 9 10"
      for i in $n10 ; do
        for j in $n10 ; do
          for i in $n10 ; do
            for j in $n10 ; do
              echo $s10 >> $out;
            done;
          done;
        done;
      done;
    '';
  });
}
