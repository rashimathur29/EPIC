'use strict';
const invoke = window.__TAURI__?.core?.invoke ?? (async (c,a) => { console.log('[DEV]',c,a); return null; });
const $ = id => document.getElementById(id);
const setText = (id,v) => { const e=$(id); if(e) e.textContent=v; };
alert('loaded');

/* ── NAV ── */
const NAV = {
    productivity:{title:'Productivity Session',icon:'fas fa-clock'},
    breaks:      {title:'Break Timers',        icon:'fas fa-hourglass-half'},
    apps:        {title:'App & URL Tracking',  icon:'fas fa-th-large'},
    calls:       {title:'Calls',               icon:'fas fa-phone'},
    settings:    {title:'Settings',            icon:'fas fa-cog'},
};
function navigate(page){
    alert('hi');
    document.querySelectorAll('.sn-item').forEach(b=>b.classList.remove('active'));
    document.querySelectorAll('.page').forEach(p=>p.classList.remove('active'));
    const btn=document.querySelector(`.sn-item[data-page="${page}"]`);
    if(btn)btn.classList.add('active');
    const pg=$(`page-${page}`); if(pg)pg.classList.add('active');
    const m=NAV[page];
    if(m){ setText('pb-title',m.title); const i=$('pb-icon'); if(i)i.className=`${m.icon} page-bar__icon`; }
    // close mobile nav
    document.querySelector('.sidenav')?.classList.remove('open');
    $('nav-overlay')?.classList.remove('show');
}
/*document.querySelectorAll('.sn-item[data-page]').forEach(btn => {
    btn.addEventListener('click', () => navigate(btn.dataset.page));
});*/
function initSidenav() {
    document.querySelectorAll('.sn-item[data-page]').forEach(btn => {
        btn.addEventListener('click', () => navigate(btn.dataset.page));
    });

    // Mobile hamburger
    document.getElementById('mob-menu-btn')?.addEventListener('click', () => {
        document.querySelector('.sidenav')?.classList.toggle('open');
        document.getElementById('nav-overlay')?.classList.toggle('show');
    });

    document.getElementById('nav-overlay')?.addEventListener('click', () => {
        document.querySelector('.sidenav')?.classList.remove('open');
        document.getElementById('nav-overlay')?.classList.remove('show');
    });
}
// Mobile hamburger
$('mob-menu-btn')?.addEventListener('click',()=>{
    document.querySelector('.sidenav')?.classList.toggle('open');
    $('nav-overlay')?.classList.toggle('show');
});
$('nav-overlay')?.addEventListener('click',()=>{
    document.querySelector('.sidenav')?.classList.remove('open');
    $('nav-overlay')?.classList.remove('show');
});


/* ── WINDOW CONTROLS ── 
const _w = () => window.__TAURI__?.window?.getCurrentWindow?.();
$('btn-minimize')?.addEventListener('click', async()=>{ try{await _w()?.minimize();}catch{} });
$('btn-maximize')?.addEventListener('click', async()=>{ try{const w=_w();if(!w)return;(await w.isMaximized())?w.unmaximize():w.maximize();}catch{} });
$('btn-close')?.addEventListener('click', async()=>{ try{await _w()?.close();}catch{} });*/

/* ── POPOVER ── */
const pop = $('popover');
$('btn-profile')?.addEventListener('click', e => {
    e.stopPropagation();
    const open = pop.classList.toggle('open');
    const r = e.currentTarget.getBoundingClientRect();
    pop.style.top = (r.bottom+4)+'px'; pop.style.right=(window.innerWidth-r.right)+'px'; pop.style.left='auto';
    e.currentTarget.setAttribute('aria-expanded', String(open));
});
//document.addEventListener('click', e => { if(!pop.contains(e.target) && e.target!==$('btn-profile')) pop.classList.remove('open'); });
//document.addEventListener('keydown', e => { if(e.key==='Escape') pop.classList.remove('open'); });

async function signout(clearOrg) {
    try{await invoke('logout');}catch{}
    sessionStorage.clear();
    window.location.replace('find_your_organization.html');
}
$('pop-signout')?.addEventListener('click', ()=>signout(false));
$('pop-signout-org')?.addEventListener('click', ()=>signout(true));
$('settings-signout')?.addEventListener('click', ()=>signout(true));
$('pop-about')?.addEventListener('click', ()=>{ pop.classList.remove('open'); alert('EPIC — Employee Productivity & Insight Console\nv1.0.0\nAiprus Software'); });
$('pop-analytics')?.addEventListener('click', async()=>{ pop.classList.remove('open'); try{await invoke('send_analytics');showBanner('Analytics sent ✓','success');}catch(e){showBanner('Failed','error');} });
$('pop-refresh')?.addEventListener('click', async()=>{ pop.classList.remove('open'); try{await invoke('refresh_data');showBanner('Refreshed ✓','success');}catch{} });
$('pop-logs')?.addEventListener('click', async()=>{ pop.classList.remove('open'); try{await invoke('send_logs');showBanner('Logs sent ✓','success');}catch{} });

/* ── BANNER ── */
function showBanner(msg, type='info', dur=5000) {
    const old=$('banner'); if(old)old.remove();
    const el=document.createElement('div'); el.id='banner';
    const c={success:'#107c10',error:'#c4314b',warn:'#d07000',info:'#6264a7'};
    Object.assign(el.style,{position:'fixed',top:'58px',left:'50%',transform:'translateX(-50%)',
        padding:'10px 24px',borderRadius:'4px',color:'#fff',fontWeight:'600',zIndex:'9999',
        boxShadow:'0 4px 12px rgba(0,0,0,.2)',fontSize:'13px',fontFamily:'var(--font)',
        background:c[type]??c.info,whiteSpace:'nowrap',transition:'opacity .4s'});
    el.textContent=msg; document.body.appendChild(el);
    if(dur>0) setTimeout(()=>{el.style.opacity='0';setTimeout(()=>el.remove(),400);},dur);
}

/* ── STOPWATCH & CHECKIN ── */
let swInt=null, activeSec=0, goalH=8;
const FMT=s=>[Math.floor(s/3600),Math.floor((s%3600)/60),s%60].map(v=>String(v).padStart(2,'0')).join(':');

async function syncSW() {
    try{activeSec=await invoke('get_total_active_seconds')??activeSec;}catch{}
    setText('stopwatchDisplay',FMT(activeSec));
    updateRing(); updateGlance();
}
function startSW(){ syncSW(); swInt=setInterval(syncSW,1000); }
function stopSW(){ clearInterval(swInt);swInt=null;activeSec=0;setText('stopwatchDisplay','00:00:00');updateRing();updateGlance(); }

function setSession(on) {
    const s=$('startStopwatch'), e=$('endStopwatch'), pill=$('sess-pill');
    if(s) s.disabled=on;
    if(e) e.disabled=!on;
    if(pill){ pill.classList.toggle('running',on); setText('sess-pill-txt',on?'Session active':'Not started'); }
}

$('startStopwatch')?.addEventListener('click', async()=>{
    const btn=$('startStopwatch');
    try{
        const running=await invoke('get_status')??false;
        if(running){showBanner('Already checked in!','warn');return;}
        if(btn){btn.disabled=true;btn.innerHTML='<i class="fas fa-spinner fa-spin"></i> Starting…';}
        await invoke('checkin');
        setSession(true); startSW(); showBanner('✅ Session started','success');
        addDailyEvent('checkin','Check-in',new Date().toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit'}));
    }catch(err){
        if(btn){btn.disabled=false;btn.innerHTML='<i class="fas fa-play"></i> Start session';}
        showBanner('Check-in failed: '+err,'error');
    }
});
$('endStopwatch')?.addEventListener('click', async()=>{
    try{
        await invoke('checkout');
        setSession(false);
        stopSW();
        showBanner('✅ Session ended — data saved','success');
        addDailyEvent('checkout','Check-out',new Date().toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit'}));
    }
    catch(e){showBanner('Check-out failed: '+e,'error');}
});

/* ── RING + GLANCE + DAILY PROGRESS ── */
const CIRC=364;

// sessionMeta accumulates from user_activity_minute data
// In production these come from: SELECT SUM(idle_seconds), SUM(keystroke_count), SUM(mouse_click_count) FROM user_activity_minute WHERE created_at >= today_start
let idleSec=0, breakSec=0, keystrokesToday=0, clicksToday=0, breaksToday=[];

function updateRing(){
    const h=activeSec/3600, pct=Math.min(h/goalH,1);
    // Active arc
    const c=$('ring-circle');
    if(c) c.style.strokeDashoffset=CIRC-(pct*CIRC);
    // Idle arc (show proportion of session that was idle)
    const totalSec=activeSec+idleSec;
    const idlePct=totalSec>0?Math.min(idleSec/totalSec,1):0;
    const ci=$('ring-idle-circle');
    if(ci) ci.style.strokeDashoffset=CIRC-(idlePct*CIRC);

    // Ring centre number
    const mins=Math.floor(activeSec/60);
    setText('ring-num', mins>=60?h.toFixed(1):mins);
    setText('ring-unit', mins>=60?'hours':'min');

    // Score badge
    const scorePct=Math.round(pct*100);
    const badge=$('score-badge'), badgeTxt=$('score-badge-txt');
    if(badge&&badgeTxt){
        badgeTxt.textContent=scorePct+'% of daily goal';
        badge.className='score-badge '+(scorePct>=80?'score-badge--high':scorePct>=40?'score-badge--mid':'score-badge--low');
    }

    // Active / Idle / Break bars (driven by real minute data)
    const total=Math.max(activeSec+idleSec+breakSec,1);
    const aPct=Math.round((activeSec/total)*100);
    const iPct=Math.round((idleSec/total)*100);
    const bPct=Math.round((breakSec/total)*100);
    const ab=$('dp-active-bar'),ib=$('dp-idle-bar'),bb=$('dp-break-bar');
    if(ab) ab.style.width=aPct+'%';
    if(ib) ib.style.width=iPct+'%';
    if(bb) bb.style.width=bPct+'%';
    setText('dp-active-label', fmtDur(activeSec));
    setText('dp-idle-label',   fmtDur(idleSec));
    setText('dp-break-label',  fmtDur(breakSec));
}

function fmtDur(s){
    if(!s||s<=0) return '—';
    const h=Math.floor(s/3600),m=Math.floor((s%3600)/60);
    return h>0?`${h}h ${m}m`:`${m}m`;
}

// Update the today's events feed inside Daily Progress card
// Sources: user_checkin (checkin_time), user_breaks (breakin_time, breakout_time)
function addDailyEvent(type, label, time, dur){
    const list=$('dp-event-list'); if(!list)return;
    // Remove placeholder if present
    const placeholder=list.querySelector('.dp-break-item');
    if(placeholder&&placeholder.textContent.includes('No events')) placeholder.remove();

    const dotClass=type==='break'?'dp-break-dot--break':'';
    const item=document.createElement('div');
    item.className='dp-break-item';
    item.innerHTML=`<span class="dp-break-dot ${dotClass}"></span>
        <span>${label}</span>
        <span style="font-size:11px;color:var(--text-muted);margin-left:4px;">${time}</span>
        ${dur?`<span class="dp-break-time">${dur}</span>`:''}`;
    list.appendChild(item);
}

function updateGlance(){
    const mins=Math.floor(activeSec/60), h=Math.floor(mins/60), m=mins%60;
    setText('g-active', h>0?`${h}h ${m}m`:`${mins}m`);
    setText('g-idle',   fmtDur(idleSec));
    setText('g-break',  fmtDur(breakSec));
    const score=Math.min(Math.round((activeSec/(goalH*3600))*100),100);
    setText('g-score', activeSec>0?score+'%':'—');
    setText('g-keys',   keystrokesToday>0?keystrokesToday.toLocaleString():'—');
    setText('g-clicks', clicksToday>0?clicksToday.toLocaleString():'—');
}

$('goal-sel')?.addEventListener('change',function(){ goalH=Number(this.value); setText('stat-goal',this.value); updateRing(); });
$('edit-goal-btn')?.addEventListener('click',()=>{ navigate('settings'); });

/* ── WEEKLY CHART — enhanced ── */
// weekData[0]=Mon … [6]=Sun, each = active hours from user_checkin
// In production: SELECT DATE(checkin_time), SUM(total_elapsed_time) FROM user_checkin GROUP BY DATE
function renderWeek(){
    const el=$('week-chart'); if(!el)return;

    // Week range label
    const now=new Date();
    const dayOfWeek=now.getDay(); // 0=Sun
    const monday=new Date(now); monday.setDate(now.getDate()-(dayOfWeek===0?6:dayOfWeek-1));
    const sunday=new Date(monday); sunday.setDate(monday.getDate()+6);
    const fmt=d=>d.toLocaleDateString('en-GB',{day:'numeric',month:'short'});
    setText('week-range',`${fmt(monday)} – ${fmt(sunday)}`);

    const days=['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];
    // Simulated data — replace with real invoke('get_weekly_active_hours') when ready
    const hrs=[7.2,6.8,8.0,5.5,0,0,0];
    const maxH=Math.max(...hrs,goalH);
    const todayIdx=dayOfWeek===0?6:dayOfWeek-1; // Mon=0

    el.innerHTML=days.map((d,i)=>{
        const isToday=(i===todayIdx);
        const hasData=hrs[i]>0;
        const pct=Math.round((hrs[i]/maxH)*100);
        const tooltip=hasData?`${hrs[i].toFixed(1)}h active`:'No session';
        return `<div class="wc-bar">
            <div class="wc-fill${isToday?' today':''}${hasData?' has-data':''}"
                 style="height:${Math.max(pct,hasData?6:3)}%;"
                 title="${d}: ${tooltip}"></div>
            <div class="wc-lbl${isToday?' today-lbl':''}">${d}</div>
        </div>`;
    }).join('');

    // Summary stats below chart
    const workedDays=hrs.filter(h=>h>0).length;
    const totalHrs=hrs.reduce((a,b)=>a+b,0);
    const avgHrs=workedDays>0?(totalHrs/workedDays):0;
    const bestIdx=hrs.indexOf(Math.max(...hrs));
    setText('ws-total', totalHrs.toFixed(1)+'h');
    setText('ws-avg',   avgHrs.toFixed(1)+'h');
    setText('ws-best',  hrs[bestIdx]>0?days[bestIdx]:'—');
    setText('ws-days',  workedDays+'/5');
}

/* ── TASKS ── */
let tasks=[];
function renderTasks(){
    const list=$('task-list'), empty=$('task-empty'); if(!list)return;
    if(!tasks.length){list.innerHTML='';if(empty)empty.style.display='';return;}
    if(empty)empty.style.display='none';
    list.innerHTML=tasks.map(t=>`
        <div class="task-item${t.done?' done':''}" data-id="${t.id}">
            <div class="task-check"></div>
            <span class="task-txt">${t.text}</span>
        </div>`).join('');
    list.querySelectorAll('.task-item').forEach(el=>{
        el.addEventListener('click',()=>{
            const t=tasks.find(x=>x.id===Number(el.dataset.id));
            if(t){t.done=!t.done;renderTasks();}
        });
    });
}
$('task-add-btn')?.addEventListener('click',()=>{ $('task-input-row')?.classList.toggle('show'); $('task-input')?.focus(); });
$('task-cancel')?.addEventListener('click',()=>$('task-input-row')?.classList.remove('show'));
$('task-save')?.addEventListener('click',()=>{
    const inp=$('task-input'), txt=inp?.value?.trim(); if(!txt)return;
    tasks.push({id:Date.now(),text:txt,done:false}); inp.value='';
    $('task-input-row')?.classList.remove('show'); renderTasks();
});
$('task-input')?.addEventListener('keydown',e=>{ if(e.key==='Enter')$('task-save')?.click(); if(e.key==='Escape')$('task-cancel')?.click(); });

/* ── BREAK TIMERS ── */
let timers=[],tseq=1,ticks={};
const FMT2=s=>[Math.floor(s/3600),Math.floor((s%3600)/60),s%60].map(v=>String(v).padStart(2,'0')).join(':');
function renderTimers(){
    const list=$('timer-list'),empty=$('timer-empty'); if(!list)return;
    if(!timers.length){list.innerHTML='';if(empty)empty.style.display='';return;}
    if(empty)empty.style.display='none';
    list.innerHTML=timers.map(t=>`
        <div class="timer-card" data-id="${t.id}">
            <div class="tc-info"><div class="tc-name">${t.name}</div><div class="tc-time">${FMT2(t.rem)}</div></div>
            <div class="tc-controls">
                <button class="icon-btn icon-btn--primary" data-a="tog" title="${t.running?'Pause':'Start'}"><i class="fas fa-${t.running?'pause':'play'}"></i></button>
                <button class="icon-btn" data-a="rst" title="Reset"><i class="fas fa-undo"></i></button>
                <button class="icon-btn icon-btn--del" data-a="del" title="Delete"><i class="fas fa-trash"></i></button>
            </div>
        </div>`).join('');
    list.querySelectorAll('.timer-card').forEach(card=>{
        const id=Number(card.dataset.id);
        card.querySelectorAll('[data-a]').forEach(btn=>{
            btn.addEventListener('click',()=>{
                if(btn.dataset.a==='tog') togTimer(id);
                if(btn.dataset.a==='rst') rstTimer(id);
                if(btn.dataset.a==='del') delTimer(id);
            });
        });
    });
}
function stepField(f,d){
    const el=$(f); if(!el)return;
    const max=f==='t-h'?23:59;
    el.value=String(((parseInt(el.value)||0)+d+max+1)%(max+1)).padStart(2,'0');
}
document.querySelectorAll('[data-f][data-s]').forEach(btn=>{
    btn.addEventListener('click',()=>stepField(btn.dataset.f,Number(btn.dataset.s)));
});
$('add-timer-btn')?.addEventListener('click',()=>{
    const h=parseInt($('t-h')?.value)||0, m=parseInt($('t-m')?.value)||0, s=parseInt($('t-s')?.value)||0;
    const tot=h*3600+m*60+s; if(tot<=0){showBanner('Set a duration first','warn');return;}
    const name=$('t-name')?.value?.trim()||`Timer ${tseq}`;
    timers.push({id:Date.now(),name,tot,rem:tot,running:false}); tseq++;
    if($('t-name'))$('t-name').value=''; renderTimers();
});
function togTimer(id){
    const t=timers.find(x=>x.id===id); if(!t)return;
    t.running=!t.running;
    if(t.running){ ticks[id]=setInterval(()=>{ t.rem=Math.max(0,t.rem-1); if(t.rem===0){clearInterval(ticks[id]);delete ticks[id];t.running=false;showBanner(`⏰ "${t.name}" finished!`,'info',6000);} renderTimers(); },1000); }
    else{ clearInterval(ticks[id]); delete ticks[id]; }
    renderTimers();
}
function rstTimer(id){const t=timers.find(x=>x.id===id);if(!t)return;clearInterval(ticks[id]);delete ticks[id];t.running=false;t.rem=t.tot;renderTimers();}
function delTimer(id){clearInterval(ticks[id]);delete ticks[id];timers=timers.filter(x=>x.id!==id);renderTimers();}

/* ── CALL CONTROLS ── */
function togCall(id,on,off){const btn=$(id);if(!btn)return;const m=btn.classList.toggle('muted');const i=btn.querySelector('i');if(i)i.className=`fas ${m?off:on}`;btn.setAttribute('aria-pressed',String(m));}
$('btn-call-video')?.addEventListener('click',()=>togCall('btn-call-video','fa-video','fa-video-slash'));
$('btn-call-audio')?.addEventListener('click',()=>togCall('btn-call-audio','fa-microphone','fa-microphone-slash'));
$('btn-call-end')?.addEventListener('click',()=>{
    if(!confirm('End the call?'))return;
    ['btn-call-video','btn-call-audio'].forEach(id=>{const btn=$(id);if(!btn)return;btn.classList.remove('muted');const i=btn.querySelector('i');if(i)i.className=`fas ${id==='btn-call-video'?'fa-video':'fa-microphone'}`;});
});

/* ── APP & URL TRACKING (simulated) ── */
function renderApps(){
    const tb=$('app-tbody'); if(!tb)return;
    const apps=[
        {n:'Visual Studio Code',ic:'fa-code',    dur:'2h 14m',pct:67,cat:'work'},
        {n:'Google Chrome',     ic:'fa-chrome',  dur:'1h 02m',pct:31,cat:'work'},
        {n:'Slack',             ic:'fa-slack',   dur:'28m',   pct:14,cat:'work'},
        {n:'Microsoft Teams',   ic:'fa-users',   dur:'15m',   pct:8, cat:'work'},
        {n:'YouTube',           ic:'fa-youtube', dur:'11m',   pct:5, cat:'other'},
    ];
    tb.innerHTML=apps.map(a=>`<tr>
        <td><span class="app-ico"><i class="fab ${a.ic}" style="font-size:10px;"></i></span>${a.n}</td>
        <td>${a.dur}</td>
        <td><div class="bar-cell"><div class="bar-mini"><div class="bar-fill" style="width:${a.pct}%"></div></div><span style="font-size:11px;color:var(--text-muted);">${a.pct}%</span></div></td>
        <td><span class="tag tag-${a.cat}">${a.cat.charAt(0).toUpperCase()+a.cat.slice(1)}</span></td>
    </tr>`).join('');
}
function renderUrls(){
    const tb=$('url-tbody'); if(!tb)return;
    const urls=[
        {u:'github.com',       dur:'48m',v:12,cat:'work'},
        {u:'stackoverflow.com',dur:'22m',v:8, cat:'work'},
        {u:'docs.rs',          dur:'18m',v:5, cat:'work'},
        {u:'youtube.com',      dur:'11m',v:3, cat:'other'},
        {u:'twitter.com',      dur:'6m', v:4, cat:'social'},
    ];
    tb.innerHTML=urls.map(u=>`<tr>
        <td><i class="fas fa-globe" style="color:var(--text-muted);margin-right:8px;font-size:11px;"></i>${u.u}</td>
        <td>${u.dur}</td><td>${u.v}</td>
        <td><span class="tag tag-${u.cat}">${u.cat.charAt(0).toUpperCase()+u.cat.slice(1)}</span></td>
    </tr>`).join('');
}

/* ── USER INFO ── */
function loadUser(){
    const first=sessionStorage.getItem('firstName')||'R', last=sessionStorage.getItem('lastName')||'M';
    const email=sessionStorage.getItem('userEmail')||'', org=sessionStorage.getItem('orgName')||'';
    const ini=((first[0]||'')+(last[0]||'')).toUpperCase();
    ['tb-avatar','pop-av','sn-av'].forEach(id=>setText(id,ini));
    setText('pop-name',`${first} ${last}`.trim());
    setText('sn-name',`${first} ${last}`.trim());
    setText('pop-email',email); setText('settings-email',email);
}

/* ── STARTUP ── */
async function startup(){
    try{
        const st=await invoke('get_startup_status');
        if(!st)return;
        if(st.has_active_session){
            setSession(true); startSW();
            // Show checkin time in Daily Progress events
            if(st.checkin_time){
                const ct=new Date(st.checkin_time);
                const timeStr=ct.toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit'});
                addDailyEvent('checkin','Check-in (resumed)',timeStr);
            }
            if(st.offline_minutes>5){
                showBanner(`⚠️ ${st.offline_minutes} min offline — marked as break`,'warn');
                // Add offline period as a break event
                addDailyEvent('break',`Offline · ${st.offline_minutes} min`,new Date().toLocaleTimeString('en-GB',{hour:'2-digit',minute:'2-digit'}), st.offline_minutes+'m');
                breakSec += st.offline_minutes * 60;
            } else {
                showBanner('Session resumed','success',3000);
            }
            try{ await invoke('resume_tracking'); }catch{}
        }
    }catch(e){ console.warn('[startup]',e); }
}

/* ── INIT ── */
document.addEventListener('DOMContentLoaded',()=>{
    loadUser();
    renderWeek();
    renderTasks();
    renderTimers();
    renderApps();
    renderUrls();
    updateRing();
    updateGlance();
    startup();
});