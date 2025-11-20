{
  description = "dupfinder-tg";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };

        # Define Postgres Version
        pg = pkgs.postgresql_15;

        dbStart = pkgs.writeShellScriptBin "db-start" ''
          # Uses env vars set by shellHook
          ${pg}/bin/pg_ctl -D $PGDATA -l $LOG_PATH -o "-k $PGDATA" start
        '';

        dbStop = pkgs.writeShellScriptBin "db-stop" ''
          ${pg}/bin/pg_ctl -D $PGDATA stop
        '';

        dbStatus = pkgs.writeShellScriptBin "db-status" ''
          ${pg}/bin/pg_ctl -D $PGDATA status
        '';
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            (rust-bin.stable.latest.default.override {
              extensions = [ "rust-src" "rust-analyzer" ];
            })

            pg
            sqlx-cli

            # Add the scripts to the path
            dbStart
            dbStop
            dbStatus
          ];

          # This script runs when you enter the shell
          shellHook = ''
            # 1. Setup a local directory for DB data
            export PGDATA=$PWD/postgres_data
            export PGHOST=$PWD/postgres_data
            export LOG_PATH=$PWD/postgres_data/LOG
            export PGDATABASE=dupfinder_tg

            # 2. Check if DB exists, if not, initialize it
            if [ ! -d $PGDATA ]; then
              echo "Initializing postgres data..."
              ${pg}/bin/initdb -D $PGDATA --no-locale --encoding=UTF8

              # Start it temporarily to create the user/db
              echo "Starting postgres for setup..."
              ${pg}/bin/pg_ctl -D $PGDATA -l $LOG_PATH -o "-k $PGDATA" start

              sleep 2
              ${pg}/bin/createdb $PGDATABASE

              # Stop it so the main hook logic takes over
              ${pg}/bin/pg_ctl -D $PGDATA stop
            fi

            # 3. Set the Environment Variable for sqlx/rust
            # Note: We use a Unix socket (via PGHOST) so no password is needed
            export DATABASE_URL="postgres://$(whoami)@localhost/$PGDATABASE?host=$PGDATA"

            export DUPFINDER_DATABASE_URL="$DATABASE_URL"

            echo "------------------------------------------------"
            echo " 🐘 Postgres Environment Ready"
            echo " Run 'db-start' to boot the database."
            echo " URL: $DATABASE_URL"
            echo "------------------------------------------------"
          '';
        };
      }
    );
}
