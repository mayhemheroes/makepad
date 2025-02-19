use {
    crate::{
        makepad_platform::*,
        app_inner::AppInner,
        app_state::AppState,
    },
    //makepad_regex::regex::Regex
};

live_register!{
    use makepad_component::theme::*;
    App: {{App}} {
        const FS_ROOT: ""
        inner: {
            window: {caption:"Makepad Studio", pass: {clear_color: (COLOR_BG_EDITOR)}}
            collab_client: {
                //bind: "127.0.0.1"
                path: (FS_ROOT)
            }
            builder_client: {
                //bind: "127.0.0.1"
                path: (FS_ROOT)
            }
        }
    }
}

#[derive(Live, LiveHook)]
pub struct App {
    inner: AppInner,
    #[rust(AppState::new())] app_state: AppState,
}

impl App {
    
    pub fn live_register(cx: &mut Cx) {
        //let regex = Regex::new("\\d(\\d{2})\\d");
        //let mut slots = vec![None; 4];
        //println!("{:?}", regex.run("xxx1234yyy", &mut slots));
        //println!("{:?}", slots);
        
        makepad_studio_component::live_register(cx);
        crate::builder::builder_client::live_register(cx);
        crate::collab_client::live_register(cx);
        crate::rust_editor::live_register(cx);
        crate::log_view::live_register(cx);
        crate::code_editor::code_editor_impl::live_register(cx);
        crate::editors::live_register(cx);
        crate::app_inner::live_register(cx);
    }
    
    pub fn new_app(cx: &mut Cx) -> Self {
        let ret = Self::new_as_main_module(cx, &module_path!(), id!(App)).unwrap();
        ret
    }
    
    pub fn handle_event(&mut self, cx: &mut Cx, event: &mut Event) {
        self.handle_live_edit_event(cx, event, id!(App));
        self.inner.handle_event(cx, event, &mut self.app_state);
    }
}
