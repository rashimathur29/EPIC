function loadComponent(id, file) {
    fetch(file)
        .then(res => res.text())
        .then(data => {
            console.log(data)
            document.querySelector(id).innerHTML = data;
        })
        .catch(err => console.error(`Error loading ${file}:`, err));
}


document.addEventListener("DOMContentLoaded", () => {

    // Layout
    loadComponent("#header", "components/header.html");
    loadComponent("#sidenav", "components/sidenav.html");

    // Pages
    loadComponent("#page-productivity", "pages/productivity.html");
    loadComponent("#page-breaks", "pages/break.html");
    loadComponent("#page-apps", "pages/app_url.html");
    loadComponent("#page-calls", "pages/call.html");
    loadComponent("#page-settings", "pages/settings.html");

});