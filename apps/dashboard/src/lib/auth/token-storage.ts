// Kada a ajiye access_token a localStorage kwata-kwata — token din yanzu
// httpOnly cookie ne, wanda JavaScript baya iya karantawa (rage hadarin
// satar token ta XSS). Ana ajiye kawai boolean flag domin UI ta san ko an
// login, ba ainihin token ba.
const AUTH_FLAG = "uptime.is_authenticated";

export function markAuthenticated() {
  if (typeof window === "undefined") return;
  localStorage.setItem(AUTH_FLAG, "1");
}

export function isMarkedAuthenticated() {
  if (typeof window === "undefined") return false;
  return localStorage.getItem(AUTH_FLAG) === "1";
}

export function clearTokens() {
  if (typeof window === "undefined") return;
  localStorage.removeItem(AUTH_FLAG);
}
