final: prev: {

  rocksdb = prev.rocksdb.overrideAttrs (oldAttrs: rec {
    version = "11.8.1";

    src = final.fetchFromGitHub {
      owner = "facebook";
      repo = oldAttrs.pname;
      rev = "v${version}";
      hash = "sha256-zjOo0H8/qA879Lcw5wHp2BZzOhl+/wYluxKD9RCfQ+0=";
    };
  });
}
