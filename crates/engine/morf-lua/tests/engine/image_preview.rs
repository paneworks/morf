use super::*;
use std::time::{Duration, Instant};
fn pump(runtime: &mut Runtime) -> Vec<IpcValue> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        runtime.poll_services();
        let result = runtime.call_ipc("done", &[]).unwrap();
        if !result.is_empty() && result != [IpcValue::Nil] {
            return result;
        }
        assert!(Instant::now() < deadline, "preview never completed");
        std::thread::sleep(Duration::from_millis(2));
    }
}
const SOURCE: &str = r##"
    local image=morf.image
    preview=image.preview('<svg xmlns="http://www.w3.org/2000/svg" width="40" height="30"><rect width="40" height="30" fill="#0000ff"/></svg>')
    local mark={type="rect",points={{x=110,y=60},{x=125,y=75}},color="#00ff00",width=4,filled=true}
    result=nil
    morf.ipc.done=function() return result end
    morf.ipc.render=function()
        result=nil
        assert(preview.render({{"annotations",{mark},100,50}},function(ok,value)
            if ok then result=value.path else result="error:"..value end
        end))
    end
    morf.ipc.close=function() preview.close() end
"##;

#[test]
fn native_canvas_uses_workers_and_the_standard_callback_contract() {
    let root = std::env::temp_dir().join(format!("morf-lua-canvas-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = root.join("canvas.png");
    let mut runtime = Runtime::default();
    let source = format!(
        r##"
        local image=morf.image
        result=nil morf.ipc.done=function() return result end
        assert(not pcall(image.compose,{{width=0,height=20,output="unused.png"}}))
        assert(not pcall(image.compose,{{width=20,height=20,output="unused.png",background="not-a-color"}}))
        assert(image.compose {{width=40,height=30,background="#12345680",output=[===[{}]===],
            on_done=function(ok,value)
                assert(ok,value) assert(value.width==40 and value.height==30)
                result=value.path
            end}})
        assert(result==nil,'canvas ran on the Lua thread')
    "##,
        output.display()
    );
    runtime.execute("canvas.lua", source.as_bytes()).unwrap();
    assert_eq!(
        pump(&mut runtime),
        vec![IpcValue::String(output.to_string_lossy().into_owned())]
    );
    assert_eq!(
        morf_image::ops::pixel_at(&output, 0, 0, 1200).unwrap(),
        [18, 52, 86, 128]
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn native_preview_publishes_shifted_annotations_and_releases_each_previous_frame() {
    let mut runtime = Runtime::default();
    runtime.execute("preview.lua", SOURCE.as_bytes()).unwrap();
    runtime.call_ipc("render", &[]).unwrap();
    let IpcValue::String(first) = pump(&mut runtime).remove(0) else {
        panic!("expected source")
    };
    assert!(first.starts_with("memory:image/preview-"));
    assert_eq!(
        morf_image::ops::pixel_at(&first, 15, 15, 1200).unwrap(),
        [0, 255, 0, 255]
    );
    assert_eq!(
        morf_image::ops::pixel_at(&first, 35, 25, 1200).unwrap(),
        [0, 0, 255, 255]
    );
    runtime.call_ipc("render", &[]).unwrap();
    let IpcValue::String(second) = pump(&mut runtime).remove(0) else {
        panic!("expected source")
    };
    assert_ne!(first, second);
    assert!(morf_image::ops::image_info(&first).is_err());
    runtime.call_ipc("close", &[]).unwrap();
    assert!(morf_image::ops::image_info(&second).is_err());
}
#[test]
fn closing_a_queued_preview_never_publishes_a_late_image() {
    let mut runtime = Runtime::default();
    runtime.execute("cancel.lua", SOURCE.as_bytes()).unwrap();
    runtime.call_ipc("render", &[]).unwrap();
    runtime.call_ipc("close", &[]).unwrap();
    let IpcValue::String(result) = pump(&mut runtime).remove(0) else {
        panic!("expected result")
    };
    assert!(result.starts_with("error:"), "{result}");
}
#[test]
fn dropping_the_runtime_releases_a_published_preview() {
    let source = {
        let mut runtime = Runtime::default();
        runtime.execute("drop.lua", SOURCE.as_bytes()).unwrap();
        runtime.call_ipc("render", &[]).unwrap();
        let IpcValue::String(source) = pump(&mut runtime).remove(0) else {
            panic!("expected source")
        };
        source
    };
    assert!(morf_image::ops::image_info(source).is_err());
}
#[test]
fn malformed_and_unbounded_annotation_input_is_rejected_before_queueing() {
    let mut runtime = Runtime::default();
    runtime
        .execute(
            "bounds.lua",
            br##"
        local image=morf.image
        local a={type="pen",points={{x=1,y=1},{x=2,y=2}},width=4,color="#aabbcc"}
        assert(image.annotation_path(a).stroke:find("M1 1",1,true))
        a.points[2].x=0/0 assert(not pcall(image.annotation_path,a))
        a.points[2].x=2 a.color="invalid" assert(not pcall(image.annotation_path,a))
        a.color="#aabbcc" a.points={} for i=1,4097 do a.points[i]={x=i,y=i} end
        assert(not pcall(image.annotation_path,a))
        local handles={}
        for i=1,4 do handles[i]=image.preview("never-read.png") end
        assert(not pcall(image.preview,"never-read.png"))
        handles[1].close() handles[5]=image.preview("never-read.png")
        for _,p in ipairs(handles) do p.close() end
    "##,
        )
        .unwrap();
}
