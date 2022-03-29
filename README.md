# ppx

Proxy TCP, Unix and named pipe connections while executing commands in parallel

## Install

`cargo install --git https://github.com/raftario/ppx` to build and install from source. Binaries are also published as github actions artifacts for most platorms (there's even a universal maxOS one).

## Why

Let's say you want to run your node app and your containers at the same time. `pxx "npm start" "docker compose up"`. Boom. Now you might go and argue you could just use concurrently or gnu parallel. And you would be absolutely right.

Now let's come up with a more contrived scenario. You want to run your node app and your containers at the same time, but the dev server for you app is hardcoded to listen on localhost port 3000. But you you wanna connect to your app from other devices. `pxx -p "192.168.0.1:8080->localhost:3000" "npm start" "docker compose up"`. Boom. Let's say, just for fun, that you also want to make the docker api accessible, for some weird reason. `pxx -p "192.168.0.1:8080->localhost:3000" -p "192.168.0.1:2375->unix:///var/run/docker.sock" "npm start" "docker compose up"`. Boom. Easy.

Now for an even more contrived example, which would obviously never happen, and cause me to write this tool. You've got a server running your app somewhere. That server is also running tailscale because that's a cool and smart and useful thing to do. You're using prisma for database stuff and the nice thing about it is that it comes with a nice web ui to explore the database. Now you'd like to give someone who is not necessarily technical access to that nice web ui cause they want to see the data but the section of the app that is going to display that data is not ready yet. Obviously you're not gonna tell them to use ssh tunnelling to access it cause they're not technical. However, that nice fancy ui has a years old open issue about being able to configure the listening address and port cause of course you can't configure those. What if I told you there's an easy way out ? `pxx -p "tailscale:8080->localhost:5555" "npx prisma studio"`. Boom. Just tell them to open `server.example.com.beta.tailscale.net:8080` in their browser and they're good to go.

Or let's say you just want to run a proxy in the background while doing some other stuff. `pxx -p "[::]:80->localhost:8080" -r "$SHELL"`. Now you've got a proxy running for the duration of a shell session. Sometimes it doesn't have to be complicated.
