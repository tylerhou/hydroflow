import asyncio
import sys

async def wrap(inner, args):
    try:
        return (await inner(args), None)
    except:
        return (None, sys.exc_info())

def run(inner, args):
    event_loop = asyncio.get_event_loop()
    task = event_loop.create_task(wrap(inner, args))
    should_cancel = False

    try:
        res = event_loop.run_until_complete(task)
    except KeyboardInterrupt:
        should_cancel = True
        # don't re-raise the exception, to give Rust a chance to gracefully shut down
        res = (None, [Exception, Exception("Received keyboard interrupt"), None])
    except:
        should_cancel = True
        cancel_reason = sys.exc_info()

        # avoid leaking references to the coroutine
        cancel_reason[1].__traceback__ = None
        res = (None, [cancel_reason[0], cancel_reason[1], None])
        del cancel_reason

    if should_cancel:
        task.cancel()
        event_loop.run_until_complete(task)

    event_loop.run_until_complete(
        asyncio.gather(*asyncio.all_tasks(loop=event_loop))
    )

    event_loop.close()

    del task
    del event_loop

    if res[1]:
        raise res[1][1].with_traceback(res[1][2])
    else:
        return res[0]
