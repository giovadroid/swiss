#!/bin/bash

export SWISS_VERSION=2.0.0
SCRIPT_FILE="$(readlink -f "$0" 2>/dev/null || readlink "$0" 2>/dev/null || echo "$0")"
export SWISS_HOME="${SCRIPT_FILE%/*}"

export CONFIG_LVL=0

export CONFIG_LVL=1
unameOut="$(uname -s)"
case "${unameOut}" in
    Linux*)     machine=Linux;;
    Darwin*)    machine=Mac;;
    CYGWIN*)    machine=Cygwin;;
    MINGW*)     machine=MinGw;;
    *)          machine="UNKNOWN:${unameOut}"
esac

if [ "$machine" == "Mac" ]
    then
    export DISTRO=MacOS
    export DEPENDENCIES=$DISTRO.sh
    echo windows
elif [ "$machine" == "Linux" ]
    then
    export DISTRO=$(cat /etc/os-release | grep -oP "ID_LIKE=\K(.*)")
    export DEPENDENCIES=$DISTRO.sh
    echo Using $DISTRO in $machine
else
    echo Not supported OS
fi

bash script/installer/distro/$DEPENDENCIES