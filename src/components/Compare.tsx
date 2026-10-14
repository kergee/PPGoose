import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { tauriApi } from "../lib/tauri";
import { PreviewInfo } from "../types";
import { createPortal } from "react-dom";

export function Compare({ recordId, onClose }: { recordId: string; onClose: () => void }) {
  const [info, setInfo] = useState<PreviewInfo>();
  const [error, setError] = useState("");
  const [split, setSplit] = useState(50);
  const [pixel, setPixel] = useState(false);
  const [beforeSize, setBeforeSize] = useState<[number, number]>([0, 0]);
  const [afterSize, setAfterSize] = useState<[number, number]>([0, 0]);
  const [viewport, setViewport] = useState<[number, number]>([640, 400]);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => { let active = true; tauriApi.getPreview(recordId).then((data) => { if(active) setInfo(data); }).catch((e) => { if(active) setError(String(e)); }); return () => {active=false;}; }, [recordId]);
  useEffect(() => {
    const observer = new ResizeObserver(([entry]) => setViewport([entry.contentRect.width, entry.contentRect.height]));
    if (box.current) observer.observe(box.current);
    return () => observer.disconnect();
  }, []);
  useEffect(() => { const listener=(e:KeyboardEvent)=>{if(e.key==="Escape")onClose();};window.addEventListener("keydown",listener);return()=>window.removeEventListener("keydown",listener); }, [onClose]);
  const fit = beforeSize[0] ? Math.min(1,(viewport[0]-16)/beforeSize[0],(viewport[1]-16)/beforeSize[1]) : 1;
  const scale = pixel ? 1 : Math.max(0.01,fit);
  const width=beforeSize[0]*scale,height=beforeSize[1]*scale;
  const dimensions=(size:[number,number])=>size[0] ? `${size[0]} × ${size[1]}` : "读取中";
  return createPortal(
    <div className="fixed inset-0 z-50 p-3 bg-black/80 flex items-center justify-center" onClick={onClose}>
      <section role="dialog" aria-modal="true" aria-label="图片对比" onClick={(e)=>e.stopPropagation()}
        className="w-full max-w-6xl h-[90vh] text-neutral-200 bg-surface-1 border border-surface-3 rounded-xl flex flex-col min-h-0">
        <header className="p-3 flex flex-wrap items-center gap-3 border-b border-surface-3 shrink-0">
          <h2 className="font-semibold mr-auto">图片对比</h2>
          <button className="text-xs bg-surface-3 px-3 py-1 rounded" onClick={()=>setPixel(false)}>适应窗口</button>
          <button className="text-xs bg-surface-3 px-3 py-1 rounded" onClick={()=>setPixel(true)} disabled={!beforeSize[0]}>原图 100%</button>
          <button aria-label="关闭对比" onClick={onClose} className="px-2 text-neutral-400">✕</button>
        </header>
        {error && <p role="alert" className="p-3 text-red-400 text-sm select-text">{error}</p>}
        <div className="px-3 py-2 text-xs text-neutral-400 flex flex-wrap justify-between gap-2 shrink-0">
          <span>左侧原图：{dimensions(beforeSize)} · {info ? `${(info.originalSize/1024).toFixed(1)} KB` : "加载中…"}</span>
          <span>右侧压缩后：{dimensions(afterSize)} · {info ? `${(info.compressedSize/1024).toFixed(1)} KB` : "加载中…"}</span>
        </div>
        <div ref={box} className="flex-1 min-h-0 overflow-auto p-2">
          {info && <div className="relative mx-auto shrink-0" style={{width:width||"100%",height:height||"100%",backgroundColor:"#e5e7eb",backgroundImage:"linear-gradient(45deg,#cbd5e1 25%,transparent 25%,transparent 75%,#cbd5e1 75%),linear-gradient(45deg,#cbd5e1 25%,transparent 25%,transparent 75%,#cbd5e1 75%)",backgroundSize:"20px 20px",backgroundPosition:"0 0,10px 10px"}}>
            <img alt="压缩后" draggable={false} src={convertFileSrc(info.afterPath)}
              onLoad={(e)=>setAfterSize([e.currentTarget.naturalWidth,e.currentTarget.naturalHeight])} onError={()=>setError("压缩后图片无法预览，请检查文件或格式支持")}
              className="absolute inset-0 w-full h-full max-w-none" />
            <div className="absolute inset-0" style={{clipPath:`inset(0 ${100-split}% 0 0)`,backgroundColor:"#e5e7eb",backgroundImage:"linear-gradient(45deg,#cbd5e1 25%,transparent 25%,transparent 75%,#cbd5e1 75%),linear-gradient(45deg,#cbd5e1 25%,transparent 25%,transparent 75%,#cbd5e1 75%)",backgroundSize:"20px 20px",backgroundPosition:"0 0,10px 10px"}}>
            <img alt="原图" draggable={false} src={convertFileSrc(info.beforePath)}
              onLoad={(e)=>setBeforeSize([e.currentTarget.naturalWidth,e.currentTarget.naturalHeight])} onError={()=>setError("原图无法预览，请检查文件")}
              className="absolute inset-0 w-full h-full max-w-none" />
            </div>
            <div className="absolute top-0 bottom-0 w-px bg-goose-500 pointer-events-none" style={{left:`${split}%`}} />
          </div>}
        </div>
        <footer className="p-3 border-t border-surface-3 shrink-0">
          <label className="flex items-center gap-3 text-xs text-neutral-400">对比位置
            <input aria-label="对比位置" type="range" min="0" max="100" value={split} onChange={(e)=>setSplit(Number(e.target.value))} className="flex-1 accent-goose-500" />
            <span className="w-10">{split}%</span>
          </label>
          <p className="text-xs text-neutral-600 mt-2">尺寸改变时按原图比例对齐。原图 100% 模式可滚动查看细节。{info?.firstFrameOnly ? "动图对比显示首帧；输出文件保留动画。" : ""}</p>
        </footer>
      </section>
    </div>, document.body
  );
}
