
function submitMultipart() {
  const fileIpt = document.getElementById("mp-file");
  const receiverIpt = document.getElementById("mp-receiver");
  const durationIpt = document.getElementById("mp-duration");

  const formData = FormData();
  const file = fileIpt.files[0];
  formData.append("image", file);
  formData.append("receiver", receiverIpt.value);
  formData.append("duration", durationIpt.value);
  // formData.append("meta", {
  //   "rece"
  // })
}


document.addEventListener("load", () => {
  const submitBtn = document.getElementById("mp-submit");
  if (submitBtn) {
    submitBtn.addEventListener("click", submitMultipart);
  }
});
