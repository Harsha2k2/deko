"use strict";(()=>{var t=class{static{this.KEY="deko-theme"}static init(){localStorage.getItem(this.KEY)==="light"&&this.apply("light"),document.getElementById("theme-toggle")?.addEventListener("click",()=>{let i=document.documentElement.getAttribute("data-theme")==="light"?"dark":"light";this.apply(i),localStorage.setItem(this.KEY,i)})}static apply(e){document.documentElement.setAttribute("data-theme",e)}};document.addEventListener("DOMContentLoaded",()=>{t.init()});var s=document.createElement("style");s.textContent=`
@keyframes spin-ani { to { transform: rotate(360deg); } }
@keyframes slideIn { from { transform: translateY(20px); opacity: 0; } to { transform: translateY(0); opacity: 1; } }
`;document.head.appendChild(s);})();
