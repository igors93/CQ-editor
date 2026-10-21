"""Entry point: ``python -m cq_worker``."""

import logging
import os
import sys
import traceback

from .protocol import Channel, redirect_std_streams


def main() -> int:
    channel = Channel.from_std_streams()
    redirect_std_streams(channel)

    # Importing CadQuery takes a few seconds; the GUI shows a "starting"
    # state until the ready message arrives.
    try:
        # A stale CASROOT from another OpenCascade install breaks OCP.
        os.environ.pop("CASROOT", None)

        from .server import ChannelLogHandler, Server
    except Exception as error:
        channel.send(
            {
                "type": "fatal",
                "message": f"Cannot load CadQuery: {type(error).__name__}: {error}",
                "detail": traceback.format_exc(),
                "executable": sys.executable,
            }
        )
        return 1

    logging.basicConfig(level=logging.INFO, handlers=[ChannelLogHandler(channel)])
    Server(channel, session=os.environ.get("CQ_EDITOR_SESSION")).serve_forever()
    return 0


if __name__ == "__main__":
    sys.exit(main())
