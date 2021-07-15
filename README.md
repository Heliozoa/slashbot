A simple Discord bot that provides a poll command using the [Slash Command API](https://discord.com/developers/docs/interactions/slash-commands).


Requires the following environment variables, which can be set in an `.env` file:
- `APPLICATION_ID` from https://discord.com/developers/applications
- `BOT_TOKEN` from https://discord.com/developers/applications/{APPLICATION_ID}/bot
- `GUILD_ID` from right-clicking on a server and clicking `Copy ID`

The program adds the poll command for the server identified by `GUILD_ID` on startup.

## Usage:
Command name: `poll`

Options:
- `options`, accepts a comma separated list
```
/poll options:a,b,c,d
```
![Example of what the output of the poll command looks like](./docs/slashbot.png)

The poll stops accepting new votes after 5 minutes.
