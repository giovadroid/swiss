#!/usr/local/bin/nu

ln -sf (echo ($nu.env.HOME) /.cargo/bin/* | str collect) /usr/local/bin/

# TODO Add chsh to nushell and also insert if not exists into /etc/shells 
