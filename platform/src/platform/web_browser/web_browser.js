import {WasmBridge} from "/makepad/platform/wasm_bridge/src/wasm_bridge.js"

export class WasmWebBrowser extends WasmBridge {
    constructor(wasm, dispatch, canvas) {
        super (wasm, dispatch);
        
        window.onbeforeunload = _=>{
            this.wasm_terminate_thread_pools();
            this.clear_memory_refs();
            for(let worker of this.workers){
                worker.terminate();
            }
        }
        this.wasm_app = this.wasm_create_app();
        this.dispatch = dispatch;
        this.canvas = canvas;
        this.handlers = {};
        this.timers = [];
        this.text_copy_response = "";
        this.web_sockets = [];
        this.window_info = {}
        this.signals = [];
        this.signal_timeout = null;
        this.workers = [];
        this.thread_stack_size = 2 * 1024 * 1024;
        this.init_detection();
    }
    
    load_deps() {
        this.to_wasm = this.new_to_wasm();
        
        this.to_wasm.ToWasmGetDeps({
            gpu_info: this.gpu_info,
            browser_info: {
                protocol: location.protocol + "",
                host: location.host + "",
                hostname: location.hostname + "",
                pathname: location.pathname + "",
                search: location.search + "",
                hash: location.hash + "",
                has_threading_support: this.wasm._has_threading_support
            }
        });
        
        this.do_wasm_pump();
        
        this.load_deps_promise.then(
            results => {
                let deps = [];
                for (let result of results) {
                    deps.push({
                        path: result.path,
                        data: result.buffer
                    })
                }
                this.update_window_info();
                this.to_wasm.ToWasmInit({
                    window_info: this.window_info,
                    deps: deps
                });
                this.do_wasm_pump();
                
                // only bind the event handlers now
                // to stop them firing into wasm early
                this.bind_mouse_and_touch();
                this.bind_keyboard();
                this.bind_screen_resize();
                this.focus_keyboard_input();
                this.to_wasm.ToWasmRedrawAll();
                this.do_wasm_pump();

                var loaders = document.getElementsByClassName('canvas_loader');
                for (var i = 0; i < loaders.length; i ++) {
                    loaders[i].parentNode.removeChild(loaders[i])
                }
            },
            error => {
                console.error("Error loading dep", error)
            }
        )
    }
    
    // from_wasm dispatch_on_app interface
    
    js_post_signal(signal_hi, signal_lo) {
        this.signals.push({signal_hi, signal_lo});
        if (this.signal_timeout === undefined) {
            this.signal_timeout = setTimeout(_ => {
                this.signal_timeout = undefined;
                for (let signal of this.signals) {
                    this.to_wasm.ToWasmSignal({signal_hi, signal_lo});
                }
                this.do_wasm_pump();
            }, 0)
        }
    }
    
    FromWasmLoadDeps(args) {
        let promises = [];
        for (let path of args.deps) {
            promises.push(fetch_path("/makepad/", path))
        }
        
        this.load_deps_promise = Promise.all(promises);
    }
    
    FromWasmStartTimer(args) {
        let timer_id = args.timer_id;
        
        for (let i = 0; i < this.timers.length; i ++) {
            if (this.timers[i].timer_id == timer_id) {
                console.error("Timer ID collision!")
                return
            }
        }
        
        var timer = {timer_id, repeats: args.repeats};
        if (args.repeats !== 0) {
            timer.sys_id = window.setInterval(e => {
                this.to_wasm.ToWasmTimerFired({timer_id});
                this.do_wasm_pump();
            }, args.interval * 1000.0);
        }
        else {
            timer.sys_id = window.setTimeout(e => {
                for (let i = 0; i < this.timers.length; i ++) {
                    let timer = this.timers[i];
                    if (timer.timer_id == timer_id) {
                        this.timers.splice(i, 1);
                        break;
                    }
                }
                this.to_wasm.ToWasmTimerFired({timer_id});
                this.do_wasm_pump();
            }, args.interval * 1000.0);
        }
        this.timers.push(timer)
    }
    
    FromWasmStopTimer(args) {
        for (let i = 0; i < this.timers.length; i ++) {
            let timer = this.timers[i];
            if (timer.timer_id == args.timer_id) {
                if (timer.repeats) {
                    window.clearInterval(timer.sys_id);
                }
                else {
                    window.clearTimeout(timer.sys_id);
                }
                this.timers.splice(i, 1);
                return
            }
        }
    }
    
    FromWasmFullScreen() {
        if (document.body.requestFullscreen) {
            document.body.requestFullscreen();
            return
        }
        if (document.body.webkitRequestFullscreen) {
            document.body.webkitRequestFullscreen();
            return
        }
        if (document.body.mozRequestFullscreen) {
            document.body.mozRequestFullscreen();
            return
        }
    }
    
    FromWasmNormalScreen() {
        if (this.canvas.exitFullscreen) {
            this.canvas.exitFullscreen();
            return
        }
        if (this.canvas.webkitExitFullscreen) {
            this.canvas.webkitExitFullscreen();
            return
        }
        if (this.canvas.mozExitFullscreen) {
            this.canvas.mozExitFullscreen();
            return
        }
    }
    
    FromWasmRequestAnimationFrame() {
        if (this.window_info.xr_is_presenting || this.req_anim_frame_id) {
            return;
        }
        this.req_anim_frame_id = window.requestAnimationFrame(time => {
            if(this.wasm == null){
                return
            }
            this.req_anim_frame_id = 0;
            if (this.xr_is_presenting) {
                return
            }
            this.to_wasm.ToWasmAnimationFrame({time: time / 1000.0});
            this.in_animation_frame = true;
            this.do_wasm_pump();
            this.in_animation_frame = false;
        })
    }
    
    FromWasmSetDocumentTitle(args) {
        document.title = args.title
    }
    
    FromWasmSetMouseCursor(args) {
        //console.log(args);
        document.body.style.cursor = web_cursor_map[args.web_cursor] || 'default'
    }
    
    FromWasmTextCopyResponse(args) {
        this.text_copy_response = args.response
    }
    
    FromWasmShowTextIME(args) {
        self.text_area_pos = args;
        this.update_text_area_pos();
    }
    
    FromWasmHideTextIME() {
        console.error("IMPLEMENTR!")
    }
    
    
    FromWasmWebSocketOpen(args) {
        let auto_reconnect = args.auto_reconnect;
        let web_socket_id = args.web_socket_id;
        let url = args.url;
        let web_socket = new WebSocket(args.url);
        web_socket.binaryType = "arraybuffer";
        this.web_sockets[args.web_socket_id] = web_socket;
        
        web_socket.onclose = e => {
            console.log("Auto reconnecting websocket");
            this.to_wasm.ToWasmWebSocketClose({web_socket_id})
            this.do_wasm_pump();
            if (auto_reconnect) {
                this.FromWasmWebSocketOpen({
                    web_socket_id,
                    auto_reconnect,
                    url
                });
            }
        }
        web_socket.onerror = e => {
            console.error("Websocket error", e);
            this.to_wasm.ToWasmWebSocketError({web_socket_id, error: "" + e})
            this.do_wasm_pump();
        }
        web_socket.onmessage = e => {
            // ok send the binary message
            this.to_wasm.ToWasmWebSocketMessage({
                web_socket_id,
                data: e.data
            })
            this.do_wasm_pump();
        }
        web_socket.onopen = e => {
            for (let item of web_socket._queue) {
                web_socket.send(item);
            }
            web_socket._queue.length = 0;
            this.to_wasm.ToWasmWebSocketOpen({web_socket_id});
            this.do_wasm_pump();
        }
        web_socket._queue = []
    }
    
    FromWasmWebSocketSend(args) {
        let web_socket = this.web_sockets[args.web_socket_id];
        if (web_socket.readyState == 0) {
            web_socket._queue.push(this.clone_data_u8(args.data))
        }
        else {
            web_socket.send(this.view_data_u8(args.data));
        }
        this.free_data_u8(args.data);
    }
    
    alloc_thread_stack(closure_ptr) {
        let tls_size = this.exports.__tls_size.value;
        tls_size += 8 - (tls_size & 7); // align it to 8 bytes
        let stack_size = this.thread_stack_size; // 2mb
        if ((tls_size + stack_size) & 7 != 0) throw new Error("stack size not 8 byte aligned");
        let tls_ptr = this.exports.wasm_thread_alloc_tls_and_stack((tls_size + stack_size) >> 3);
        this.update_array_buffer_refs();
        let stack_ptr = tls_ptr + tls_size + stack_size - 8;
        return {
            tls_ptr,
            stack_ptr,
            bytes: this.wasm._bytes,
            memory: this.wasm._memory,
            closure_ptr
        }
    }
    
    // thanks to JP Posma with Zaplib for figuring out how to do the stack_pointer export without wasm bindgen
    // https://github.com/Zaplib/zaplib/blob/650305c856ea64d9c2324cbd4b8751ffbb971ac3/zaplib/cargo-zaplib/src/build.rs#L48
    // https://github.com/Zaplib/zaplib/blob/7cb3bead16f963e60c840aa2be3bf28a47ac533e/zaplib/web/common.ts#L313
    // And Ingvar Stepanyan for https://web.dev/webassembly-threads/
    // example build command:
    // RUSTFLAGS="-C target-feature=+atomics,+bulk-memory,+mutable-globals -C link-arg=--export=__stack_pointer" cargo build -p thing_to_compile --target=wasm32-unknown-unknown -Z build-std=panic_abort,std
    FromWasmCreateThread(args) {
        let worker = new Worker(
            '/makepad/platform/src/platform/web_browser/web_worker.js',
            {type: 'module'}
        );
        
        if (!this.wasm._has_thread_support) {
            console.error("FromWasmCreateThread not available, wasm file not compiled with threading support");
            return
        }
        if (this.exports.__stack_pointer === undefined) {
            console.error("FromWasmCreateThread not available, wasm file not compiled with -C link-arg=--export=__stack_pointer");
            return
        }
        
        worker.postMessage(this.alloc_thread_stack(args.closure_ptr));
        
        worker.addEventListener("message", (e) => {
            // this one collects signals to stop swamping the main thread
            this.signals.push(e.data);
            if(this.signal_timeout === null){
                this.signal_timeout = setTimeout(_=>{
                    this.signal_timeout = null;
                    let signals = this.signals;
                    this.signals = [];
                    this.to_wasm.ToWasmSignal({signals})
                    this.do_wasm_pump();
                },1)
            }
        })
        
        this.workers.push(worker);
    }
    
    FromWasmSpawnAudioOutput(args) {
        
        if (!this.audio_context) {
            const start_audio = async () => {
                let context = this.audio_context = new AudioContext();
                
                await context.audioWorklet.addModule("/makepad/platform/src/platform/web_browser/audio_worklet.js", {credentials: 'omit'});
                
                const audio_worklet = new AudioWorkletNode(context, 'audio-worklet', {
                    numberOfInputs: 0,
                    numberOfOutputs: 1,
                    outputChannelCount: [2],
                    processorOptions: {thread_info: this.alloc_thread_stack(args.closure_ptr)}
                });
                
                audio_worklet.port.onmessage = (e) => {
                    let data = e.data;
                    switch (data.message_type) {
                        case "console_log":
                        console.log(data.value);
                        break;
                        
                        case "console_error":
                        console.error(data.value);
                        break;
                        
                        case "signal":
                        this.to_wasm.ToWasmSignal(data)
                        this.do_wasm_pump();
                        break;
                    }
                };
                audio_worklet.onprocessorerror = (err) => {
                    console.error(err);
                }
                audio_worklet.connect(context.destination);
                return audio_worklet;
            };
            
            start_audio();
            let user_interact_hook = () => {
                this.audio_context.resume();
            }
            window.addEventListener('click', user_interact_hook)
            window.addEventListener('touchstart', user_interact_hook)
        }
    }
    
    FromWasmStartMidiInput() {
        navigator.requestMIDIAccess().then((midi) => {
            
            let reload_midi_ports = () => {
                
                let inputs = [];
                let input_id = 0;
                for (let input_pair of midi.inputs) {
                    let input = input_pair[1];
                    inputs.push({
                        uid: "" + input.id,
                        name: input.name,
                        manufacturer: input.manufacturer,
                    });
                    input.onmidimessage = (e) => {
                        let data = e.data;
                        this.to_wasm.ToWasmMidiInputData({
                            input_id: input_id,
                            data: (data[0] << 16) | (data[1] << 8) | data[2],
                        });
                        this.do_wasm_pump();
                    }
                    input_id += 1;
                }
                this.to_wasm.ToWasmMidiInputList({inputs});
                this.do_wasm_pump();
            }
            midi.onstatechange = (e) => {
                reload_midi_ports();
            }
            reload_midi_ports();
        }, () => {
            console.error("Cannot open midi");
        });
    }
    
    // calling into wasm
    
    
    wasm_terminate_thread_pools() {
        this.exports.wasm_terminate_thread_pools(this.wasm_app);
    }
    
    wasm_create_app() {
        let new_ptr = this.exports.wasm_create_app();
        this.update_array_buffer_refs();
        return new_ptr
    }
    
    wasm_process_msg(to_wasm) {
        let ret_ptr = this.exports.wasm_process_msg(to_wasm.release_ownership(), this.wasm_app)
        this.update_array_buffer_refs();
        return this.new_from_wasm(ret_ptr);
    }
    
    do_wasm_pump() {
        let to_wasm = this.to_wasm;
        this.to_wasm = this.new_to_wasm();
        let from_wasm = this.wasm_process_msg(to_wasm);
        from_wasm.dispatch_on_app();
        from_wasm.free();
    }
    
    
    // init and setup
    
    
    init_detection() {
        this.detect = {
            user_agent: window.navigator.userAgent,
            is_mobile_safari: window.navigator.platform.match(/iPhone|iPad/i),
            is_touch_device: ('ontouchstart' in window || navigator.maxTouchPoints),
            is_firefox: navigator.userAgent.toLowerCase().indexOf('firefox') > -1,
            use_touch_scroll_overlay: window.ontouchstart === null
        }
        this.detect.is_android = this.detect.user_agent.match(/Android/i)
        this.detect.is_add_to_homescreen_safari = this.is_mobile_safari && navigator.standalone
    }
    
    update_window_info() {
        var dpi_factor = window.devicePixelRatio;
        var w;
        var h;
        var canvas = this.canvas;
        
        if (this.window_info.xr_is_presenting) {
            let xr_webgllayer = this.xr_session.renderState.baseLayer;
            this.dpi_factor = 3.0;
            this.width = 2560.0 / this.dpi_factor;
            this.height = 2000.0 / this.dpi_factor;
        }
        else {
            if (canvas.getAttribute("fullpage")) {
                if (this.detect.is_add_to_homescreen_safari) { // extremely ugly. but whatever.
                    if (window.orientation == 90 || window.orientation == -90) {
                        h = screen.width;
                        w = screen.height - 90;
                    }
                    else {
                        w = screen.width;
                        h = screen.height - 80;
                    }
                }
                else {
                    w = window.innerWidth;
                    h = window.innerHeight;
                }
            }
            else {
                w = canvas.offsetWidth;
                h = canvas.offsetHeight;
            }
            var sw = canvas.width = w * dpi_factor;
            var sh = canvas.height = h * dpi_factor;
            
            this.gl.viewport(0, 0, sw, sh);
            
            this.window_info.dpi_factor = dpi_factor;
            this.window_info.inner_width = canvas.offsetWidth;
            this.window_info.inner_height = canvas.offsetHeight;
            // send the wasm a screenresize event
        }
        this.window_info.is_fullscreen = is_fullscreen();
        this.window_info.can_fullscreen = can_fullscreen();
    }
    
    bind_screen_resize() {
        this.window_info = {};
        
        this.handlers.on_screen_resize = () => {
            this.update_window_info();
            if (this.to_wasm !== undefined) {
                this.to_wasm.ToWasmResizeWindow({window_info: this.window_info});
                this.FromWasmRequestAnimationFrame();
            }
        }
        
        // TODO! BIND THESE SOMEWHERE USEFUL
        this.handlers.on_app_got_focus = () => {
            this.to_wasm.ToWasmAppGotFocus();
            this.do_wasm_pump();
        }
        
        this.handlers.on_app_lost_focus = () => {
            this.to_wasm.ToWasmAppGotFocus();
            this.do_wasm_pump();
        }
        
        window.addEventListener('resize', _ => this.handlers.on_screen_resize())
        window.addEventListener('orientationchange', _ => this.handlers.on_screen_resize())
    }
    
    bind_mouse_and_touch() {
        
        var canvas = this.canvas
        let last_mouse_finger;
        if (this.detect.use_touch_scroll_overlay) {
            var ts = this.touch_scroll_overlay = document.createElement('div')
            ts.className = "makepad_webgl_scroll_overlay"
            var ts_inner = document.createElement('div')
            var style = document.createElement('style')
            style.innerHTML = "\n"
                + "div.makepad_webgl_scroll_overlay {\n"
                + "z-index: 10000;\n"
                + "margin:0;\n"
                + "overflow:scroll;\n"
                + "top:0;\n"
                + "left:0;\n"
                + "width:100%;\n"
                + "height:100%;\n"
                + "position:fixed;\n"
                + "background-color:transparent\n"
                + "}\n"
                + "div.cx_webgl_scroll_overlay div{\n"
                + "margin:0;\n"
                + "width:400000px;\n"
                + "height:400000px;\n"
                + "background-color:transparent\n"
                + "}\n"
            
            document.body.appendChild(style)
            ts.appendChild(ts_inner);
            document.body.appendChild(ts);
            canvas = ts;
            
            ts.scrollTop = 200000;
            ts.scrollLeft = 200000;
            let last_scroll_top = ts.scrollTop;
            let last_scroll_left = ts.scrollLeft;
            let scroll_timeout = null;
            
            this.handlers.on_overlay_scroll = e => {
                let new_scroll_top = ts.scrollTop;
                let new_scroll_left = ts.scrollLeft;
                let dx = new_scroll_left - last_scroll_left;
                let dy = new_scroll_top - last_scroll_top;
                last_scroll_top = new_scroll_top;
                last_scroll_left = new_scroll_left;
                
                window.clearTimeout(scroll_timeout);
                
                scroll_timeout = window.setTimeout(_ => {
                    ts.scrollTop = 200000;
                    ts.scrollLeft = 200000;
                    last_scroll_top = ts.scrollTop;
                    last_scroll_left = ts.scrollLeft;
                }, 200);
                
                let finger = last_mouse_finger;
                if (finger) {
                    finger.is_touch = false;
                    this.to_wasm.ToWasmFingerScroll({
                        finger: finger,
                        scroll_x: dx,
                        scroll_y: dy
                    });
                    this.do_wasm_pump();
                }
            }
            
            ts.addEventListener('scroll', e => this.handlers.on_overlay_scroll(e))
        }
        
        var mouse_fingers = [];
        function mouse_to_finger(e) {
            let mf = mouse_fingers[e.button] || (mouse_fingers[e.button] = {});
            mf.x = e.pageX;
            mf.y = e.pageY;
            mf.digit = e.button;
            mf.time = e.timeStamp / 1000.0;
            mf.modifiers = pack_key_modifier(e);
            mf.touch = false;
            return mf
        }
        
        var digit_map = {}
        var digit_alloc = 0;
        
        function touch_to_finger_alloc(e) {
            var f = []
            for (let i = 0; i < e.changedTouches.length; i ++) {
                var t = e.changedTouches[i]
                // find an unused digit
                var digit = undefined;
                for (digit in digit_map) {
                    if (!digit_map[digit]) break
                }
                // we need to alloc a new one
                if (digit === undefined || digit_map[digit]) digit = digit_alloc ++;
                // store it
                digit_map[digit] = {identifier: t.identifier};
                // return allocated digit
                digit = parseInt(digit);
                
                f.push({
                    x: t.pageX,
                    y: t.pageY,
                    digit: digit,
                    time: e.timeStamp / 1000.0,
                    modifiers: 0,
                    touch: true,
                })
            }
            return f
        }
        
        function lookup_digit(identifier) {
            for (let digit in digit_map) {
                var digit_id = digit_map[digit]
                if (!digit_id) continue
                if (digit_id.identifier == identifier) {
                    return digit
                }
            }
        }
        
        function touch_to_finger_lookup(e) {
            var f = []
            for (let i = 0; i < e.changedTouches.length; i ++) {
                var t = e.changedTouches[i]
                f.push({
                    x: t.pageX,
                    y: t.pageY,
                    digit: lookup_digit(t.identifier),
                    time: e.timeStamp / 1000.0,
                    modifiers: {},
                    touch: true,
                })
            }
            return f
        }
        
        function touch_to_finger_free(e) {
            var f = []
            for (let i = 0; i < e.changedTouches.length; i ++) {
                var t = e.changedTouches[i]
                var digit = lookup_digit(t.identifier)
                if (!digit) {
                    console.log("Undefined state in free_digit");
                    digit = 0
                }
                else {
                    digit_map[digit] = undefined
                }
                
                f.push({
                    x: t.pageX,
                    y: t.pageY,
                    time: e.timeStamp / 1000.0,
                    digit: digit,
                    modifiers: 0,
                    touch: true,
                })
            }
            return f
        }
        
        var mouse_buttons_down = [];
        
        this.handlers.on_mouse_down = e => {
            e.preventDefault();
            this.focus_keyboard_input();
            mouse_buttons_down[e.button] = true;
            this.to_wasm.ToWasmFingerDown({finger: mouse_to_finger(e)});
            this.do_wasm_pump();
        }
        
        this.handlers.on_mouse_up = e => {
            e.preventDefault();
            mouse_buttons_down[e.button] = false;
            this.to_wasm.ToWasmFingerUp({finger: mouse_to_finger(e)});
            this.do_wasm_pump();
        }
        
        this.handlers.on_mouse_move = e => {
            document.body.scrollTop = 0;
            document.body.scrollLeft = 0;
            
            for (var i = 0; i < mouse_buttons_down.length; i ++) {
                if (mouse_buttons_down[i]) {
                    let mf = mouse_to_finger(e);
                    mf.digit = i;
                    this.to_wasm.ToWasmFingerMove({finger: mf});
                }
            }
            last_mouse_finger = mouse_to_finger(e);
            this.to_wasm.ToWasmFingerHover({finger: last_mouse_finger});
            this.do_wasm_pump();
            //console.log("Redraw cycle "+(end-begin)+" ms");
        }
        
        this.handlers.on_mouse_out = e => {
            this.to_wasm.ToWasmFingerOut({finger: mouse_to_finger(e)});
            this.do_wasm_pump();
        }
        
        canvas.addEventListener('mousedown', e => this.handlers.on_mouse_down(e))
        window.addEventListener('mouseup', e => this.handlers.on_mouse_up(e))
        window.addEventListener('mousemove', e => this.handlers.on_mouse_move(e));
        window.addEventListener('mouseout', e => this.handlers.on_mouse_out(e));
        
        this.handlers.on_contextmenu = e => {
            e.preventDefault()
            return false
        }
        
        canvas.addEventListener('contextmenu', e => this.handlers.on_contextmenu(e))
        
        this.handlers.on_touchstart = e => {
            e.preventDefault()
            
            let fingers = touch_to_finger_alloc(e);
            for (let i = 0; i < fingers.length; i ++) {
                this.to_wasm.ToWasmFingerDown({finger: fingers[i]});
            }
            this.do_wasm_pump();
            return false
        }
        
        this.handlers.on_touchmove = e => {
            //e.preventDefault();
            var fingers = touch_to_finger_lookup(e);
            for (let i = 0; i < fingers.length; i ++) {
                this.to_wasm.ToWasmFingerMove({finger: fingers[i]});
            }
            this.do_wasm_pump();
            return false
        }
        
        this.handlers.on_touch_end_cancel_leave = e => {
            e.preventDefault();
            var fingers = touch_to_finger_free(e);
            for (let i = 0; i < fingers.length; i ++) {
                this.to_wasm.ToWasmFingerUp({finger: fingers[i]});
            }
            this.do_wasm_pump();
            return false
        }
        
        canvas.addEventListener('touchstart', e => this.handlers.on_touchstart(e))
        canvas.addEventListener('touchmove', e => this.handlers.on_touchmove(e), {passive: false})
        canvas.addEventListener('touchend', e => this.handlers.on_touch_end_cancel_leave(e));
        canvas.addEventListener('touchcancel', e => this.handlers.on_touch_end_cancel_leave(e));
        canvas.addEventListener('touchleave', e => this.handlers.on_touch_end_cancel_leave(e));
        
        var last_wheel_time;
        var last_was_wheel;
        this.handlers.on_mouse_wheel = e => {
            var finger = mouse_to_finger(e)
            e.preventDefault()
            let delta = e.timeStamp - last_wheel_time;
            last_wheel_time = e.timeStamp;
            // typical web bullshit. this reliably detects mousewheel or touchpad on mac in safari
            if (this.detect.is_firefox) {
                last_was_wheel = e.deltaMode == 1
            }
            else { // detect it
                if (Math.abs(Math.abs((e.deltaY / e.wheelDeltaY)) - (1. / 3.)) < 0.00001 || !last_was_wheel && delta < 250) {
                    last_was_wheel = false;
                }
                else {
                    last_was_wheel = true;
                }
            }
            //console.log(e.deltaY / e.wheelDeltaY);
            //last_delta = delta;
            var fac = 1
            if (e.deltaMode === 1) fac = 40
            else if (e.deltaMode === 2) fac = window.offsetHeight
            
            finger.is_touch = !last_was_wheel;
            this.to_wasm.ToWasmFingerScroll({
                finger: finger,
                scroll_x: e.deltaX * fac,
                scroll_y: e.deltaY * fac
            });
            this.do_wasm_pump();
        };
        canvas.addEventListener('wheel', e => this.handlers.on_mouse_wheel(e))
    }
    
    bind_keyboard() {
        if (this.detect.is_mobile_safari || this.detect.is_android) { // mobile keyboards are unusable on a UI like this. Not happening.
            return
        }
        
        var ta = this.text_area = document.createElement('textarea')
        ta.className = "cx_webgl_textinput"
        ta.setAttribute('autocomplete', 'off')
        ta.setAttribute('autocorrect', 'off')
        ta.setAttribute('autocapitalize', 'off')
        ta.setAttribute('spellcheck', 'false')
        var style = document.createElement('style')
        
        style.innerHTML = "\n"
            + "textarea.cx_webgl_textinput {\n"
            + "z-index: 1000;\n"
            + "position: absolute;\n"
            + "opacity: 0;\n"
            + "border-radius: 4px;\n"
            + "color:white;\n"
            + "font-size: 6;\n"
            + "background: gray;\n"
            + "-moz-appearance: none;\n"
            + "appearance:none;\n"
            + "border:none;\n"
            + "resize: none;\n"
            + "outline: none;\n"
            + "overflow: hidden;\n"
            + "text-indent: 0px;\n"
            + "padding: 0 0px;\n"
            + "margin: 0 -1px;\n"
            + "text-indent: 0px;\n"
            + "-ms-user-select: text;\n"
            + "-moz-user-select: text;\n"
            + "-webkit-user-select: text;\n"
            + "user-select: text;\n"
            + "white-space: pre!important;\n"
            + "}\n"
            + "textarea: focus.cx_webgl_textinput {\n"
            + "outline: 0px !important;\n"
            + "-webkit-appearance: none;\n"
            + "}"
        
        document.body.appendChild(style)
        ta.style.left = -100 + 'px'
        ta.style.top = -100 + 'px'
        ta.style.height = 1
        ta.style.width = 1
        
        //document.addEventListener('focusout', this.onFocusOut.bind(this))
        var was_paste = false;
        this.neutralize_ime = false;
        var last_len = 0;
        
        this.handlers.on_cut = e => {
            setTimeout(_ => {
                ta.value = "";
                last_len = 0;
            }, 0)
        }
        
        ta.addEventListener('cut', e => this.handlers.on_cut(e));
        
        this.handlers.on_copy = e => {
            setTimeout(_ => {
                ta.value = "";
                last_len = 0;
            }, 0)
        }
        
        ta.addEventListener('copy', e => this.handlers.on_copy(e));
        
        this.handlers.on_paste = e => {
            was_paste = true;
        }
        
        ta.addEventListener('paste', e => this.handlers.on_paste(e));
        
        this.handlers.on_select = e => {}
        
        ta.addEventListener('select', e => this.handlers.on_select(e))
        
        this.handlers.on_input = e => {
            if (ta.value.length > 0) {
                if (was_paste) {
                    was_paste = false;
                    
                    this.to_wasm.text_input({
                        was_paste: true,
                        input: ta.value.substring(last_len),
                        replace_last: false,
                    })
                    ta.value = "";
                }
                else {
                    var replace_last = false;
                    var text_value = ta.value;
                    if (ta.value.length >= 2) { // we want the second char
                        text_value = ta.value.substring(1, 2);
                        ta.value = text_value;
                    }
                    else if (ta.value.length == 1 && last_len == ta.value.length) { // its an IME replace
                        replace_last = true;
                    }
                    // we should send a replace last
                    if (replace_last || text_value != '\n') {
                        this.to_wasm.ToWasmTextInput({
                            was_paste: false,
                            input: text_value,
                            replace_last: replace_last,
                        });
                    }
                }
                this.do_wasm_pump();
            }
            last_len = ta.value.length;
        };
        ta.addEventListener('input', e => this.handlers.on_input(e));
        
        ta.addEventListener('mousedown', e => this.handlers.on_mouse_down(e));
        ta.addEventListener('mouseup', e => this.handlers.on_mouse_up(e));
        ta.addEventListener('wheel', e => this.handlers.on_mouse_wheel(e));
        
        ta.addEventListener('contextmenu', e => this.handlers.on_contextmenu(e));
        
        ta.addEventListener('blur', e => {
            this.focus_keyboard_input();
        })
        
        var ugly_ime_hack = false;
        
        this.handlers.on_keydown = e => {
            let code = e.keyCode;
            
            //if (code == 91) {firefox_logo_key = true; e.preventDefault();}
            if (code == 18 || code == 17 || code == 16) e.preventDefault(); // alt
            if (code === 8 || code === 9) e.preventDefault() // backspace/tab
            if ((code === 88 || code == 67) && (e.metaKey || e.ctrlKey)) { // copy or cut
                // we need to request the clipboard
                this.to_wasm.ToWasmTextCopy();
                this.do_wasm_pump();
                ta.value = this.text_copy_response;
                ta.selectionStart = 0;
                ta.selectionEnd = ta.value.length;
            }
            //    this.keyboardCut = true // x cut
            //if(code === 65 && (e.metaKey || e.ctrlKey)) this.keyboardSelectAll = true     // all (select all)
            if (code === 89 && (e.metaKey || e.ctrlKey)) e.preventDefault() // all (select all)
            if (code === 83 && (e.metaKey || e.ctrlKey)) e.preventDefault() // ctrl s
            if (code === 90 && (e.metaKey || e.ctrlKey)) {
                this.update_text_area_pos();
                ta.value = "";
                ugly_ime_hack = true;
                ta.readOnly = true;
                e.preventDefault()
            }
            // if we are using arrow keys, home or end
            let key_code = e.keyCode;
            
            if (key_code >= 33 && key_code <= 40) {
                ta.value = "";
                last_len = ta.value.length;
            }
            //if(key_code
            this.to_wasm.ToWasmKeyDown({key: {
                key_code: key_code,
                char_code: e.charCode,
                is_repeat: e.repeat,
                time: e.timeStamp / 1000.0,
                modifiers: pack_key_modifier(e)
            }})
            
            this.do_wasm_pump();
        };
        
        ta.addEventListener('keydown', e => this.handlers.on_keydown(e));
        
        this.handlers.on_keyup = e => {
            let code = e.keyCode;
            
            if (code == 18 || code == 17 || code == 16) e.preventDefault(); // alt
            if (code == 91) {e.preventDefault();}
            var ta = this.text_area;
            if (ugly_ime_hack) {
                ugly_ime_hack = false;
                document.body.removeChild(ta);
                this.bind_keyboard();
                this.update_text_area_pos();
            }
            this.to_wasm.ToWasmKeyUp({key: {
                key_code: e.keyCode,
                char_code: e.charCode,
                is_repeat: e.repeat,
                time: e.timeStamp / 1000.0,
                modifiers: pack_key_modifier(e)
            }})
            this.do_wasm_pump();
        };
        ta.addEventListener('keyup', e => this.handlers.on_keyup(e));
        document.body.appendChild(ta);
        ta.focus();
    }
    
    
    // internal helper api
    
    
    update_text_area_pos() {
        if (!this.text_area)return;
        var pos = this.text_area_pos;
        var ta = this.text_area;
        if (ta && pos) {
            ta.style.left = (Math.round(pos.x) - 4) + "px";
            ta.style.top = Math.round(pos.y) + "px"
        }
    }
    
    
    focus_keyboard_input() {
        if (!this.text_area)return;
        this.text_area.focus();
    }
    
}

function can_fullscreen() {
    return (document.fullscreenEnabled || document.webkitFullscreenEnabled || document.mozFullscreenEnabled)? true: false
}

function is_fullscreen() {
    return (document.fullscreenElement || document.webkitFullscreenElement || document.mozFullscreenElement)? true: false
}

function fetch_path(base, path) {
    return new Promise(function(resolve, reject) {
        var req = new XMLHttpRequest()
        req.addEventListener("error", function() {
            reject(resource)
        })
        req.responseType = 'arraybuffer'
        req.addEventListener("load", function() {
            if (req.status !== 200) {
                return reject(req.status)
            }
            resolve({
                path: path,
                buffer: req.response
            })
        })
        req.open("GET", base + path)
        req.send()
    })
}

let web_cursor_map = [
    "none", //Hidden=>0
    "default", //Default=>1,
    "crosshair", //CrossHair=>2,
    "pointer", //Hand=>3,
    "default", //Arrow=>4,
    "move", //Move=>5,
    "text", //Text=>6,
    "wait", //Wait=>7,
    "help", //Help=>8,
    "not-allowed", //NotAllowed=>9,
    "n-resize", // NResize=>10,
    "ne-resize", // NeResize=>11,
    "e-resize", // EResize=>12,
    "se-resize", // SeResize=>13,
    "s-resize", // SResize=>14,
    "sw-resize", // SwResize=>15,
    "w-resize", // WResize=>16,
    "nw-resize", // NwResize=>17,
    "ns-resize", //NsResize=>18,
    "nesw-resize", //NeswResize=>19,
    "ew-resize", //EwResize=>20,
    "nwse-resize", //NwseResize=>21,
    "col-resize", //ColResize=>22,
    "row-resize", //RowResize=>23,
]

//var firefox_logo_key = false;
function pack_key_modifier(e) {
    return (e.shiftKey? 1: 0) | (e.ctrlKey? 2: 0) | (e.altKey? 4: 0) | (e.metaKey? 8: 0)
}
