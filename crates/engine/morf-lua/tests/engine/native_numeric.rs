//! Reference numbers were exported from the previous Lua implementations.
//! This checks the migration against prior behaviour, not a duplicate port.
use super::*;
#[test]
fn native_geometry_and_spectrum_match_previous_numeric_results() {
    let mut runtime = Runtime::default();
    let reference = include_str!("fixtures/native_numeric.json");
    let source = format!(
        r#"
        local expected=morf.json.decode([====[{reference}]====])
        local g=morf.geometry
        local vertices={{{{-2,-1,.23}},{{1.3,-.7,.19}},{{1.7,.5,.41}},{{-.3,1.4,.12}},{{-1.6,.3,.33}}}}
        local curves=g.shape_curves(g.polygon(vertices),97)
        assert(#curves==#expected.polygon)
        for i,c in ipairs(curves) do for k,v in ipairs(c) do assert(math.abs(v-expected.polygon[i][k])<1e-9,'cubic '..i..' coordinate '..k) end end
        for _,case in ipairs(expected.audio) do
            local f=morf.audio.spectrum_filter(case.options)
            for i,frame in ipairs(case.frames) do
                local bars=f.step(frame.bands,frame.dt)
                for k,v in ipairs(bars) do assert(math.abs(v-frame.bars[k])<1e-11,'frame '..i..' bar '..k) end
                assert(math.abs(f.gain()-frame.gain)<1e-11)
            end
        end
        assert(g.graph_series({{0,50,100}},{{samples=5,width=100,height=40,top=100,bottom=0,closed=false}})=='M50.0 39.0 L75.0 20.0 L100.0 1.0')
        assert(g.graph_series({{10,15,20}},{{samples=3,width=100,height=40,top=20,bottom=10,closed=true}})=='M0.0 39.0 L50.0 20.0 L100.0 1.0 L100.0 40.0 L0.0 40.0 Z')
        -- Byte rates: readings in the millions map into the box like any others.
        assert(g.graph_series({{0,5e7,1e8}},{{samples=3,width=100,height=40,top=1e8,bottom=0,closed=false}})=='M0.0 39.0 L50.0 20.0 L100.0 1.0')
        assert(g.graph_grid(100,40,2,2)=='M50.0 0 V40.0 M0 20.0 H100.0')
        local original={{{{0,0,0,0,1,1,1,1}}}}
        g.shape_curves(original,false) assert(original[1][1]==0,'input was mutated')
        assert(not pcall(g.shape_path,'missing'))
        assert(not pcall(g.shape_path,'circle',{{segments=0}}))
        assert(not pcall(g.polygon,{{{{1,1}},{{1,1}},{{1,1}}}},{{rounding=0/0}}))
        assert(not pcall(g.graph_series,{{0,1}},{{samples=0}}))
        assert(not pcall(morf.audio.spectrum_filter,{{bars=0}}))
        assert(not pcall(morf.audio.spectrum_filter,{{smoothing=1.1}}))
    "#
    );
    runtime.execute("reference.lua", source.as_bytes()).unwrap();
}
