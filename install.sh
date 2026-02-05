#!/bin/sh
set -e

BASE_URL="https://github.com/freecomputinglab/rheo"
INSTALL_DIR=$HOME/.local/bin
PROJECT_NAME=rheo

download() {
  cd $(mktemp -d)

  echo 'Downloading prebuilt binary from Github...'
  curl --silent --location "${BASE_URL}/releases/latest/download/$1.zip" --output $1.zip
  unzip $1.zip

  mkdir -p $INSTALL_DIR
  mv $PROJECT_NAME $INSTALL_DIR/$PROJECT_NAME
}

ARCH=$(uname -m)
OS=$(uname -o)

pick_target() {
  echo "Selecting target for $ARCH / $OS..."

  if [ -n "$1" ]; then
    cargo install $PROJECT_NAME --locked --git $BASE_URL --rev $1
    return
  elif [ "$OS" = "Linux" ] || [ "$OS" = "GNU/Linux" ]; then
    if [ "$ARCH" = "x86_64" ]; then
      download "x86_64-unknown-linux-gnu"
      return
    fi
  elif [ "$OS" = "Darwin" ]; then
    if [ "$ARCH" = "arm64" ]; then
      download "aarch64-apple-darwin"
      return
    elif [ "$ARCH" = "x86_64" ]; then
      download "x86_64-apple-darwin"
      return
    fi
  elif [ "$OS" = "Msys" ]; then
    if [ "$ARCH" = "arm64" ]; then
      download "aarch64-pc-windows-msvc"
      return
    elif [ "$ARCH" = "x86_64" ]; then
      download "x86_64-pc-windows-msvc"
      return
    fi
  fi

  echo 'Prebuilt binary not available, installing from source. This may take a few minutes.'
  cargo install $PROJECT_NAME --locked
}

pick_target $1

echo '$PROJECT_NAME installation is complete.'
