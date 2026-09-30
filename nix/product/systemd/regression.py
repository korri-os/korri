"""Real PID1 regression. All mutations use native systemd DBus methods."""
import asyncio
import os
import sys
from pathlib import Path
from dbus_next import BusType, DBusError
from dbus_next.aio import MessageBus

DEST = 'org.freedesktop.systemd1'
ROOT = '/org/freedesktop/systemd1'
UNIT = DEST + '.Unit'

async def main():
    bus = await MessageBus(bus_type=BusType.SYSTEM).connect()
    obj = bus.get_proxy_object(DEST, ROOT, await bus.introspect(DEST, ROOT))
    manager = obj.get_interface(DEST + '.Manager')
    results = {}
    changed = asyncio.Event()

    def removed(job_id, path, unit, result):
        results[path] = result
        changed.set()
    manager.on_job_removed(removed)
    await manager.call_subscribe()

    async def finished(path, expected='done'):
        async with asyncio.timeout(20):
            while path not in results:
                changed.clear()
                if path not in results:
                    await changed.wait()
        assert results[path] == expected, (path, results[path], expected)

    async def job(method, unit, expected='done'):
        await finished(await getattr(manager, 'call_' + method + '_unit')(unit, 'replace'), expected)

    async def prop(unit, field, interface=UNIT):
        path = await manager.call_get_unit(unit)
        obj = bus.get_proxy_object(DEST, path, await bus.introspect(DEST, path))
        return (await obj.get_interface('org.freedesktop.DBus.Properties').call_get(interface, field)).value

    async def until(unit, field, predicate, interface=UNIT):
        path = await manager.call_get_unit(unit)
        obj = bus.get_proxy_object(DEST, path, await bus.introspect(DEST, path))
        properties = obj.get_interface('org.freedesktop.DBus.Properties')
        event = asyncio.Event()
        def change(interface, values, invalidated):
            event.set()
        properties.on_properties_changed(change)
        try:
            async with asyncio.timeout(10):
                while True:
                    event.clear()
                    if predicate((await properties.call_get(interface, field)).value):
                        return
                    await event.wait()
        finally:
            properties.off_properties_changed(change)

    async def state(unit, expected):
        await until(unit, 'FreezerState', lambda value: value == expected)

    async def failure(call, suffix):
        try:
            await call
        except DBusError as exc:
            assert exc.type.endswith(suffix), (exc.type, suffix)
        else:
            raise AssertionError('DBus call unexpectedly succeeded: ' + suffix)

    def marker(name):
        path = Path('/var/lib/freezer-test') / name
        return path.read_text() if path.exists() else ''

    if len(sys.argv) > 1 and sys.argv[1] == 'unprivileged':
        # Only unit-specific stop/restart rights, not start/thaw or RefUnit.
        await failure(manager.call_thaw_unit('kiosk.service'), 'AccessDenied')
        await failure(manager.call_stop_unit('other.service', 'replace'), 'InteractiveAuthorizationRequired')
        await state('kiosk.service', 'frozen')
        await job(sys.argv[2], 'kiosk.service')
        print('PASS: narrow unit/verb polkit rights; no external thaw permission')
        return

    await job('start', 'kiosk.service')
    old = await prop('kiosk.service', 'MainPID', DEST + '.Service')
    await manager.call_freeze_unit('kiosk.service')
    await job('start', 'kiosk.service')
    await state('kiosk.service', 'frozen')
    await job('reload', 'kiosk.service', 'frozen')
    await state('kiosk.service', 'frozen')
    print('PASS: start stays a no-op and reload stays rejected while frozen')
    await job('restart', 'kiosk.service')
    assert str(old) + ' SIGTERM' in marker('kiosk')
    assert old != await prop('kiosk.service', 'MainPID', DEST + '.Service')
    await state('kiosk.service', 'running')
    await manager.call_freeze_unit('kiosk.service')
    await job('stop', 'kiosk.service')
    assert len(marker('kiosk').splitlines()) == 2
    print('PASS: first-attempt frozen restart and stop deliver SIGTERM')

    # Deterministic in-flight freeze: FUSE keeps a task in a kernel wait.
    await job('start', 'inflight.service')
    await asyncio.to_thread(Path('/run/freezer-read-ready').read_text)
    freeze = asyncio.create_task(manager.call_freeze_unit('inflight.service'))
    await state('inflight.service', 'freezing')
    assert not freeze.done(), 'fixture did not hold an outstanding FreezeUnit request'
    stop_path = await manager.call_stop_unit('inflight.service', 'replace')
    await failure(freeze, 'FreezeCancelled')
    await asyncio.to_thread(Path('/run/freezer-read-release').write_text, 'release\n')
    await finished(stop_path)
    assert 'SIGTERM' in marker('inflight')
    print('PASS: outstanding FreezeUnit canceled; synchronous thaw still stops')

    # Queue freeze/stop and thaw/stop together. Every stop is submitted once.
    for i in range(30):
        await job('start', 'kiosk.service')
        if i % 2:
            await manager.call_freeze_unit('kiosk.service')
            freezer = asyncio.create_task(manager.call_thaw_unit('kiosk.service'))
        else:
            freezer = asyncio.create_task(manager.call_freeze_unit('kiosk.service'))
        stop = asyncio.create_task(job('stop', 'kiosk.service'))
        result = await asyncio.gather(freezer, stop, return_exceptions=True)
        assert result[1] is None, result
        if isinstance(result[0], DBusError):
            assert result[0].type.rsplit('.', 1)[-1] in ('FreezeCancelled', 'UnitBusy', 'UnitInactive'), result
        else:
            assert result[0] is None, result
    assert len(marker('kiosk').splitlines()) == 32
    print('PASS: 30 freeze/thaw versus stop races; all calls completed')

    await job('start', 'held.service')
    os.mkfifo('/run/freezer-release')
    await manager.call_freeze_unit('held.service')
    stop_path = await manager.call_stop_unit('held.service', 'replace')
    await failure(manager.call_freeze_unit('held.service'), 'UnitBusy')
    await failure(manager.call_thaw_unit('held.service'), 'UnitBusy')
    # Opening the FIFO synchronizes with ExecStop, without time-based retries.
    await asyncio.to_thread(Path('/run/freezer-release').write_text, 'release\n')
    await finished(stop_path)
    assert 'SIGTERM' in marker('held')
    print('PASS: external freeze and thaw still reject pending stop jobs')

    await job('start', 'child.service')
    await job('start', 'sibling.service')
    await manager.call_freeze_unit('parent.slice')
    await state('child.service', 'frozen-by-parent')
    await job('stop', 'child.service', 'frozen')
    await manager.call_freeze_unit('child.service')
    await job('stop', 'child.service', 'frozen')
    await state('parent.slice', 'frozen')
    await state('sibling.service', 'frozen-by-parent')
    assert not marker('child') and not marker('sibling')
    await manager.call_thaw_unit('parent.slice')
    await state('child.service', 'frozen')
    await job('stop', 'child.service')
    await job('stop', 'sibling.service')
    print('PASS: no ancestor thaw; directly frozen child survives parent thaw')

    await job('start', 'watchdog.service')
    await manager.call_freeze_unit('watchdog.service')
    # Deliberately exceed WatchdogSec; this is test stimulus, not build polling.
    await asyncio.sleep(5)
    await state('watchdog.service', 'frozen')
    await job('restart', 'watchdog.service')
    await asyncio.sleep(5)
    assert await prop('watchdog.service', 'ActiveState') == 'active'
    await manager.call_freeze_unit('watchdog.service')
    await job('stop', 'watchdog.service')
    assert len(marker('watchdog').splitlines()) == 2
    print('PASS: watchdog paused while frozen, reset by stop thaw and restart')

    # Real product dependency lists, fixture processes: an actual SIGKILL of
    # the korrid stand-in must stop and then pull in the frozen kiosk once.
    unit = 'korri-chromium-kiosk.service'
    old = await prop(unit, 'MainPID', DEST + '.Service')
    assert old > 0
    await manager.call_freeze_unit(unit)
    await manager.call_kill_unit('korrid.service', 'main', 9)
    await until(unit, 'MainPID', lambda value: value not in (0, old), DEST + '.Service')
    await until(unit, 'ActiveState', lambda value: value == 'active')
    await state(unit, 'running')
    assert str(old) + ' SIGTERM' in marker('recovery-kiosk')
    assert await prop('korrid.service', 'NRestarts', DEST + '.Service') == 1
    print('PASS: actual product dependency wiring recovers frozen kiosk after korrid fixture SIGKILL')

    # Leave this directly frozen for the driver to power off the VM, not stop it.
    await job('start', 'shutdown-marker.service')
    await manager.call_freeze_unit('shutdown-marker.service')
    await job('start', 'kiosk.service')
    await manager.call_freeze_unit('kiosk.service')
    print('PASS: shutdown marker armed')

asyncio.run(asyncio.wait_for(main(), 180))
